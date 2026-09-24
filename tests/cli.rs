use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

// 返回当前测试构建生成的 mkd 二进制路径。
fn mkd() -> &'static str {
    env!("CARGO_BIN_EXE_mkd")
}

// 创建用于隔离 CLI 输出的唯一临时目录。
fn temporary_directory() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("mkd-e2e-{unique}"));
    fs::create_dir_all(&path).unwrap();
    path
}

// 返回仓库内指定分类和名称的示例目录。
fn example(category: &str, name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(category)
        .join(name)
}

// 使用指定工作目录执行 mkd 并捕获输出。
fn run(current_directory: &Path, arguments: &[&str]) -> Output {
    Command::new(mkd())
        .current_dir(current_directory)
        .args(arguments)
        .output()
        .unwrap()
}

// 验证从项目深层目录运行时能够向上发现根文件并编译。
#[test]
fn binary_discovers_main_from_a_nested_directory() {
    let output_directory = temporary_directory();
    let artifact = output_directory.join("plan.md");
    let project = example("valid", "09-deep-directory");
    let result = run(
        &project.join("org/product/api"),
        &["expose_service", "-o", artifact.to_str().unwrap()],
    );

    assert!(result.status.success(), "{:?}", result);
    assert!(result.stdout.is_empty());
    let logs = String::from_utf8(result.stderr).unwrap();
    assert!(logs.contains("Parsing <root>"));
    assert!(logs.contains("Compiling expose_service"));
    assert!(logs.contains("Finished built target `expose_service`"));
    let markdown = fs::read_to_string(&artifact).unwrap();
    assert!(markdown.contains("## 阶段 5：最终目标"));
    assert!(markdown.contains("### 目标 `org::product::api::handler::publish_api`"));
    fs::remove_dir_all(output_directory).unwrap();
}

// 验证显式根文件允许从项目外编译任意命名空间目标。
#[test]
fn binary_compiles_a_namespaced_target_with_an_explicit_root() {
    let working_directory = temporary_directory();
    let artifact = working_directory.join("identity.md");
    let root_file = example("valid", "10-enterprise-platform").join("main.mf");
    let result = run(
        &working_directory,
        &[
            "platform::services::identity::ready",
            "--root",
            root_file.to_str().unwrap(),
            "-o",
            artifact.to_str().unwrap(),
        ],
    );

    assert!(result.status.success(), "{:?}", result);
    let markdown = fs::read_to_string(&artifact).unwrap();
    assert!(markdown.contains("# 规格实施计划：platform::services::identity::ready"));
    assert!(markdown.contains("### 目标 `platform::infra::database::database_ready`"));
    fs::remove_dir_all(working_directory).unwrap();
}

// 验证模板路径相对工作目录并将多阶段目标渲染为 Markdown。
#[test]
fn binary_builds_with_a_custom_template_from_the_working_directory() {
    let working_directory = temporary_directory();
    let root = example("valid", "09-deep-directory").join("main.mf");
    let template = "# {{ plan.root_target }}\n{% for stage in plan.stages %}## 阶段 {{ stage.index }}\n{% for target in stage.targets %}- {{ target.id }}: {{ target.description }}\n{% for spec in target.specifications %}  - [ ] {{ spec }}\n{% endfor %}{% endfor %}{% endfor %}";
    fs::write(working_directory.join("custom.md"), template).unwrap();
    let result = run(
        &working_directory,
        &[
            "expose_service",
            "--root",
            root.to_str().unwrap(),
            "--template",
            "custom.md",
            "-o",
            "result.md",
        ],
    );

    assert!(result.status.success(), "{result:?}");
    let markdown = fs::read_to_string(working_directory.join("result.md")).unwrap();
    assert!(markdown.starts_with("# expose_service\n## 阶段 1\n"));
    assert!(markdown.contains("- org::product::api::handler::publish_api:"));
    assert!(markdown.contains("  - [ ] "));
    fs::remove_dir_all(working_directory).unwrap();
}

// 验证检查模式校验模板却不写输出并保持单独检查的行为。
#[test]
fn binary_check_with_template_validates_rendering_without_writing() {
    let working_directory = temporary_directory();
    let root = example("valid", "01-single-target").join("main.mf");
    let template = working_directory.join("plan.md");
    fs::write(&template, "{{ plan.root_target }}").unwrap();
    let args = [
        "greet",
        "--root",
        root.to_str().unwrap(),
        "--check",
        "--template",
        "plan.md",
    ];
    let valid = run(&working_directory, &args);
    assert!(valid.status.success(), "{valid:?}");
    assert_eq!(
        fs::read_to_string(&template).unwrap(),
        "{{ plan.root_target }}"
    );
    assert!(!working_directory.join("result.md").exists());
    fs::write(&template, "{{ plan.missing }}").unwrap();
    let invalid = run(&working_directory, &args);
    assert!(!invalid.status.success());
    let stderr = String::from_utf8_lossy(&invalid.stderr);
    assert!(stderr.contains("plan.md`:1:"), "{stderr}");
    assert!(stderr.contains("undefined value"), "{stderr}");
    let plain_check = run(
        &working_directory,
        &["greet", "--root", root.to_str().unwrap(), "--check"],
    );
    assert!(plain_check.status.success(), "{plain_check:?}");
    fs::remove_dir_all(working_directory).unwrap();
}

// 验证模板语法错误和外部模板引用不能生成或覆盖输出。
#[test]
fn binary_rejects_invalid_templates_without_overwriting_output() {
    let project = temporary_directory();
    fs::write(
        project.join("main.mf"),
        "---\n# build\n- accepted\n---\n> build\n",
    )
    .unwrap();
    fs::write(project.join("layout.md"), "{% for stage in plan.stages %}").unwrap();
    fs::write(project.join("result.md"), "keep").unwrap();
    let result = run(
        &project,
        &["build", "--template", "layout.md", "-o", "result.md"],
    );
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("layout.md`:1:"));
    assert_eq!(
        fs::read_to_string(project.join("result.md")).unwrap(),
        "keep"
    );
    fs::write(project.join("layout.md"), "{% include 'secret.md' %}").unwrap();
    let result = run(
        &project,
        &["build", "--template", "layout.md", "-o", "result.md"],
    );
    assert!(!result.status.success());
    assert_eq!(
        fs::read_to_string(project.join("result.md")).unwrap(),
        "keep"
    );
    let missing = run(
        &project,
        &["build", "--template", "missing.md", "-o", "result.md"],
    );
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("missing.md"));
    assert_eq!(
        fs::read_to_string(project.join("result.md")).unwrap(),
        "keep"
    );
    fs::remove_dir_all(project).unwrap();
}

// 验证编译产物不能覆盖模板、根文件或分析过的依赖模块。
#[test]
fn binary_refuses_to_overwrite_compilation_inputs() {
    let project = temporary_directory();
    fs::write(
        project.join("main.mf"),
        "> crate::shared::ready\n---\n# build\n> ready\n---\n> build\n",
    )
    .unwrap();
    fs::write(
        project.join("shared.mf"),
        "---\n# ready\n- yes\n---\n> ready\n",
    )
    .unwrap();
    fs::write(project.join("layout.md"), "# {{ plan.root_target }}\n").unwrap();
    for file in ["main.mf", "shared.mf", "layout.md"] {
        let before = fs::read_to_string(project.join(file)).unwrap();
        let result = run(&project, &["build", "--template", "layout.md", "-o", file]);
        assert!(!result.status.success(), "{file}: {result:?}");
        assert!(String::from_utf8_lossy(&result.stderr).contains("would overwrite"));
        assert_eq!(fs::read_to_string(project.join(file)).unwrap(), before);
    }
    let before = fs::read_to_string(project.join("shared.mf")).unwrap();
    let no_template = run(&project, &["build", "-o", "shared.mf"]);
    assert!(!no_template.status.success());
    assert_eq!(
        fs::read_to_string(project.join("shared.mf")).unwrap(),
        before
    );
    fs::remove_dir_all(project).unwrap();
}

// 验证 init 生成的项目可从子目录按默认配方构建，且支持多个命名入口。
#[test]
fn init_and_named_profiles_build_reproducibly_from_nested_directory() {
    let directory = temporary_directory();
    let project = directory.join("project");
    let initialized = run(&directory, &["init", "project"]);
    assert!(initialized.status.success(), "{initialized:?}");
    fs::create_dir_all(project.join("nested")).unwrap();
    let default = run(&project.join("nested"), &["profile"]);
    assert!(default.status.success(), "{default:?}");
    assert!(
        fs::read_to_string(project.join("dist/plan.md"))
            .unwrap()
            .contains("规格实施计划：start")
    );
    fs::write(
        project.join("guide.mf"),
        "---\n# write\n- 文档已完成。\n---\n> write\n",
    )
    .unwrap();
    fs::write(project.join("mkd.toml"), "version = 1\n[build]\ndefault = 'main'\n[build.main]\ntarget = 'start'\noutput = 'dist/plan.md'\n[build.guide]\ntarget = 'guide::write'\noutput = 'dist/guide.md'\n").unwrap();
    let named = run(&project.join("nested"), &["profile", "guide"]);
    assert!(named.status.success(), "{named:?}");
    assert!(
        fs::read_to_string(project.join("dist/guide.md"))
            .unwrap()
            .contains("guide::write")
    );
    fs::remove_dir_all(directory).unwrap();
}

// 验证配置模板与 CLI 覆盖选项、检查模式不写出制品。
#[test]
fn profile_uses_configured_template_and_check_respects_no_template() {
    let directory = temporary_directory();
    fs::write(
        directory.join("main.mf"),
        "---\n# start\n- 完成。\n---\n> start\n",
    )
    .unwrap();
    fs::write(directory.join("layout.md.j2"), "{{ plan.missing }}").unwrap();
    fs::write(directory.join("mkd.toml"), "version = 1\n[build]\ndefault = 'main'\n[build.main]\ntarget = 'start'\noutput = 'dist/plan.md'\ntemplate = 'layout.md.j2'\n").unwrap();
    let checked = run(&directory, &["profile", "--check"]);
    assert!(!checked.status.success());
    assert!(!directory.join("dist").exists());
    assert!(
        run(&directory, &["profile", "--check", "--no-template"])
            .status
            .success()
    );
    fs::write(
        directory.join("layout.md.j2"),
        "# 自定义：{{ plan.root_target }}\n",
    )
    .unwrap();
    let built = run(&directory, &["profile"]);
    assert!(built.status.success(), "{built:?}");
    assert_eq!(
        fs::read_to_string(directory.join("dist/plan.md")).unwrap(),
        "# 自定义：start"
    );
    fs::write(
        directory.join("alternate.md.j2"),
        "# 替代：{{ plan.root_target }}",
    )
    .unwrap();
    let override_template = run(
        &directory,
        &[
            "profile",
            "--template",
            "alternate.md.j2",
            "-o",
            "custom/alternate.md",
        ],
    );
    assert!(override_template.status.success(), "{override_template:?}");
    assert_eq!(
        fs::read_to_string(directory.join("custom/alternate.md")).unwrap(),
        "# 替代：start"
    );
    let override_output = run(
        &directory,
        &["profile", "-o", "custom/output.md", "--no-template"],
    );
    assert!(override_output.status.success(), "{override_output:?}");
    assert!(
        fs::read_to_string(directory.join("custom/output.md"))
            .unwrap()
            .contains("规格实施计划：start")
    );
    fs::remove_dir_all(directory).unwrap();
}

// 验证旧 Target 命令不读配置，并允许名为 build 的 Target。
#[test]
fn direct_target_command_remains_independent_of_configuration() {
    let directory = temporary_directory();
    fs::write(
        directory.join("main.mf"),
        "---\n# build\n- done\n---\n> build\n",
    )
    .unwrap();
    fs::write(directory.join("mkd.toml"), "version = 9\n").unwrap();
    let direct = run(&directory, &["build", "-o", "output/nested.md"]);
    assert!(direct.status.success(), "{direct:?}");
    assert!(directory.join("output/nested.md").exists());
    assert!(!run(&directory, &["profile"]).status.success());
    assert!(run(&directory, &["build", "--check"]).status.success());
    fs::remove_dir_all(directory).unwrap();
}

// 验证 init 补齐已有 Markfile 需显式可编译目标且不会覆盖既有文件。
#[test]
fn init_safely_completes_existing_project() {
    let directory = temporary_directory();
    let source = "---\n# publish\n- ready\n---\n> publish\n";
    fs::write(directory.join("main.mf"), source).unwrap();
    assert!(!run(&directory, &["init"]).status.success());
    assert!(
        !run(&directory, &["init", "--target", "missing"])
            .status
            .success()
    );
    assert!(!directory.join("mkd.toml").exists());
    let init = run(&directory, &["init", "--target", "publish"]);
    assert!(init.status.success(), "{init:?}");
    assert!(run(&directory, &["profile"]).status.success());
    assert_eq!(
        fs::read_to_string(directory.join("main.mf")).unwrap(),
        source
    );
    assert!(
        !run(&directory, &["init", "--target", "publish"])
            .status
            .success()
    );
    fs::remove_dir_all(directory).unwrap();
}

// 验证配置的未知字段、无效默认配方和逃逸路径不会被忽略。
#[test]
fn profile_rejects_invalid_configuration_and_escaping_paths() {
    let directory = temporary_directory();
    fs::write(
        directory.join("main.mf"),
        "---\n# start\n- done\n---\n> start\n",
    )
    .unwrap();
    for config in [
        "version = 1\n[build]\ndefault = 'missing'\n[build.main]\ntarget = 'start'\noutput = 'dist/plan.md'\n",
        "version = 1\n[build]\ndefault = 'main'\n[build.main]\ntarget = 'start'\noutput = '../outside.md'\n",
        "version = 1\n[build]\ndefault = 'main'\n[build.main]\ntarget = 'start'\noutput = 'dist/plan.md'\nunknown = 2\n",
    ] {
        fs::write(directory.join("mkd.toml"), config).unwrap();
        let result = run(&directory, &["profile"]);
        assert!(!result.status.success(), "{result:?}");
        assert!(String::from_utf8_lossy(&result.stderr).contains("invalid config"));
    }
    assert!(!directory.join("dist/plan.md").exists());
    fs::remove_dir_all(directory).unwrap();
}

// 验证配方即使命令行覆盖了输出，也不能覆盖源配置文件。
#[test]
fn profile_output_cannot_overwrite_mkd_toml() {
    let directory = temporary_directory();
    let init = run(&directory, &["init"]);
    assert!(init.status.success(), "{init:?}");
    let config = fs::read_to_string(directory.join("mkd.toml")).unwrap();
    let result = run(&directory, &["profile", "-o", "mkd.toml"]);
    assert!(!result.status.success());
    assert_eq!(
        fs::read_to_string(directory.join("mkd.toml")).unwrap(),
        config
    );
    fs::remove_dir_all(directory).unwrap();
}

// 验证检查模式以失败状态返回完整项目诊断。
#[test]
fn binary_check_reports_invalid_project_diagnostics() {
    let project = example("invalid", "10-dependency-cycle");
    let result = run(&project, &["first", "--check"]);

    assert!(!result.status.success());
    let stderr = String::from_utf8(result.stderr).unwrap();
    assert!(stderr.contains("Error "));
    assert!(stderr.contains("[P014]"));
    assert!(stderr.contains("dependency cycle"));
    assert!(result.stdout.is_empty());
}

// 验证非法目标参数返回失败状态和明确消息。
#[test]
fn binary_rejects_invalid_target_syntax() {
    let project = example("valid", "01-single-target");
    let result = run(&project, &["bad::", "--check"]);

    assert_eq!(result.status.code(), Some(1));
    let stderr = String::from_utf8(result.stderr).unwrap();
    assert!(stderr.contains("Error invalid target `bad::`"));
}

// 验证管道默认无颜色且颜色选项可以显式覆盖。
#[test]
fn binary_color_mode_controls_ansi_sequences() {
    let project = example("valid", "01-single-target");
    let plain = run(&project, &["greet", "--check"]);
    let colored = run(&project, &["greet", "--check", "--color", "always"]);
    let never = run(&project, &["greet", "--check", "--color", "never"]);

    assert!(plain.status.success());
    assert!(colored.status.success());
    assert!(never.status.success());
    assert!(!plain.stderr.contains(&0x1b));
    assert!(colored.stderr.starts_with(b"\x1b[36m"));
    assert!(
        colored
            .stderr
            .windows(6)
            .any(|window| window == b"\x1b[1;36")
    );
    assert!(!never.stderr.contains(&0x1b));
}

// 验证单文件 lint 警告不阻塞成功，全量 lint 检查未引用文件。
#[test]
fn binary_lint_scopes_and_exit_codes() {
    let project = temporary_directory();
    fs::write(
        project.join("main.mf"),
        "---\n# start\n- ready\n---\n> start\n",
    )
    .unwrap();
    fs::write(project.join("extra.mf"), "---\n# unused\n---\n> unused\n").unwrap();
    let single = run(&project, &["lint", "extra.mf", "--color", "never"]);
    assert!(single.status.success(), "{single:?}");
    assert!(String::from_utf8_lossy(&single.stderr).contains("[L001]"));

    fs::write(
        project.join("extra.mf"),
        "---\n# unused\n> missing\n---\n> unused\n",
    )
    .unwrap();
    let all = run(&project, &["lint", "--all", "--color", "never"]);
    assert_eq!(all.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&all.stderr).contains("[P005]"));
    fs::remove_dir_all(project).unwrap();
}

// 验证格式化写回、检查模式、幂等与错误文件保护。
#[test]
fn binary_fmt_checks_and_writes_without_changing_invalid_files() {
    let project = temporary_directory();
    let file = project.join("draft.mf");
    fs::write(&file, "---\n##  task  \ntext  \n-  spec  \n---\n>  task\n").unwrap();
    let check = run(&project, &["fmt", "draft.mf", "--check"]);
    assert_eq!(check.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&check.stderr).contains("needs formatting"));
    let format = run(&project, &["fmt", "draft.mf"]);
    assert!(format.status.success(), "{format:?}");
    let once = fs::read_to_string(&file).unwrap();
    assert!(once.contains("# task\ntext\n- spec"));
    assert!(
        run(&project, &["fmt", "draft.mf", "--check"])
            .status
            .success()
    );
    assert!(run(&project, &["fmt", "draft.mf"]).status.success());
    assert_eq!(once, fs::read_to_string(&file).unwrap());

    fs::write(&file, "invalid\n").unwrap();
    let invalid = run(&project, &["fmt", "draft.mf"]);
    assert_eq!(invalid.status.code(), Some(1));
    assert_eq!(fs::read_to_string(&file).unwrap(), "invalid\n");
    fs::remove_dir_all(project).unwrap();
}

// 验证编译模式缺少输出参数时由 clap 返回用法错误。
#[test]
fn binary_requires_output_outside_check_mode() {
    let project = example("valid", "01-single-target");
    let result = run(&project, &["greet"]);

    assert_eq!(result.status.code(), Some(2));
    let stderr = String::from_utf8(result.stderr).unwrap();
    assert!(stderr.contains("--output <FILE>"));
}
