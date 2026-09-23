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
