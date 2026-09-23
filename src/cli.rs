use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use clap::{CommandFactory, Parser, Subcommand, error::ErrorKind};

use crate::compiler::{CompileError, Compiler, DefaultMarkdownRenderer};
use crate::core::{ModulePath, ModulePathError, TargetId};
use crate::formatter::{FormatError, Formatter};
use crate::linter::Linter;
use crate::logging::{ColorChoice, Logger};
use crate::parser::{ParseMode, Parser as MarkfileParser};
use crate::project::{AnalysisMode, ProjectAnalyzer};

// 定义 mkd 的命令行参数。
#[derive(Debug, Parser)]
#[command(
    name = "mkd",
    version,
    about = "Analyze and compile Markfile specifications",
    args_conflicts_with_subcommands = true
)]
struct Cli {
    #[arg(
        value_name = "TARGET",
        help = "Target to check or compile (target or module::target)"
    )]
    target: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,

    #[arg(
        short,
        long,
        value_name = "FILE",
        conflicts_with = "check",
        help = "Write the compiled Markdown artifact to FILE"
    )]
    output: Option<PathBuf>,

    #[arg(
        long,
        value_name = "MARKFILE",
        global = true,
        help = "Use MARKFILE as the project root instead of searching for main.mf"
    )]
    root: Option<PathBuf>,

    #[arg(long, help = "Analyze the project without writing an artifact")]
    check: bool,

    #[arg(long, value_enum, default_value_t = ColorChoice::Auto, global = true, help = "Control ANSI colors in progress and diagnostic logs")]
    color: ColorChoice,
}

// 定义独立于目标构建的维护命令。
#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "Lint a Markfile or every Markfile in the project")]
    Lint {
        #[arg(
            required_unless_present = "all",
            conflicts_with = "all",
            help = "Markfile to inspect"
        )]
        file: Option<PathBuf>,
        #[arg(long, help = "Inspect all Markfiles under the project root")]
        all: bool,
    },
    #[command(about = "Format a Markfile without changing its specifications")]
    Fmt {
        #[arg(help = "Markfile to format")]
        file: PathBuf,
        #[arg(long, help = "Check formatting without writing the file")]
        check: bool,
    },
}

/// 描述命令执行期间无法由项目诊断表达的错误。
#[derive(Debug)]
enum CliError {
    CurrentDirectory(io::Error),
    RootNotFound(PathBuf),
    RootIsDirectory(PathBuf),
    InvalidTarget(String),
    AnalysisFailed,
    ProjectUnavailable,
    OutputRequired,
    OutputAliasesRoot(PathBuf),
    Compilation(CompileError),
    InvalidArguments,
    FormatRejected,
    FormatChangedMeaning,
    FormatMismatch(PathBuf),
    ReadFile { path: PathBuf, source: io::Error },
    WriteOutput { path: PathBuf, source: io::Error },
    WriteDiagnostics(io::Error),
}

impl CliError {
    // 判断详细错误是否已由结构化诊断输出。
    fn diagnostics_reported(&self) -> bool {
        matches!(self, Self::AnalysisFailed | Self::FormatRejected)
    }
}

impl fmt::Display for CliError {
    // 将命令行错误渲染为面向用户的文本。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CurrentDirectory(error) => {
                write!(formatter, "cannot read current directory: {error}")
            }
            Self::RootNotFound(start) => write!(
                formatter,
                "no main.mf found in `{}` or its parents; use --root to specify a root Markfile",
                start.display()
            ),
            Self::RootIsDirectory(path) => {
                write!(
                    formatter,
                    "root path `{}` must be a Markfile",
                    path.display()
                )
            }
            Self::InvalidTarget(target) => write!(
                formatter,
                "invalid target `{target}`; expected `target` or `module::target`"
            ),
            Self::AnalysisFailed => formatter.write_str("project analysis failed"),
            Self::ProjectUnavailable => {
                formatter.write_str("analysis succeeded without a project model")
            }
            Self::OutputRequired => formatter.write_str("compilation requires an output file"),
            Self::OutputAliasesRoot(path) => write!(
                formatter,
                "output path `{}` would overwrite the root Markfile",
                path.display()
            ),
            Self::Compilation(error) => write!(formatter, "compilation failed: {error}"),
            Self::InvalidArguments => {
                formatter.write_str("provide a TARGET and either --check or -o FILE")
            }
            Self::FormatRejected => formatter.write_str("formatting rejected due to syntax errors"),
            Self::FormatChangedMeaning => {
                formatter.write_str("formatting would change parsed meaning; file left untouched")
            }
            Self::FormatMismatch(path) => {
                write!(formatter, "`{}` needs formatting", path.display())
            }
            Self::ReadFile { path, source } => {
                write!(formatter, "cannot read `{}`: {source}", path.display())
            }
            Self::WriteOutput { path, source } => {
                write!(formatter, "cannot write `{}`: {source}", path.display())
            }
            Self::WriteDiagnostics(error) => write!(formatter, "cannot write logs: {error}"),
        }
    }
}

impl Error for CliError {}

/// 解析进程参数并执行 mkd 命令。
pub fn run() -> ExitCode {
    let cli = Cli::parse();
    if cli.command.is_none() && (cli.target.is_none() || (!cli.check && cli.output.is_none())) {
        let message = if cli.target.is_none() {
            "the following required arguments were not provided:\n  <TARGET>"
        } else {
            "the following required arguments were not provided:\n  --output <FILE>"
        };
        let _ = Cli::command()
            .error(ErrorKind::MissingRequiredArgument, message)
            .print();
        return ExitCode::from(2);
    }
    let terminal_allows_color =
        io::stderr().is_terminal() && env::var_os("NO_COLOR").is_none_or(|value| value.is_empty());
    let color = cli.color.enabled(terminal_allows_color);
    let mut logger = Logger::new(io::stderr().lock(), color);
    let current_directory = match env::current_dir() {
        Ok(directory) => directory,
        Err(error) => {
            let _ = logger.error(&CliError::CurrentDirectory(error).to_string());
            return ExitCode::FAILURE;
        }
    };
    match execute(cli, &current_directory, &mut logger) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if !error.diagnostics_reported() {
                let _ = logger.error(&error.to_string());
            }
            ExitCode::FAILURE
        }
    }
}

// 执行已解析的命令并将日志写入注入的记录器。
fn execute(
    cli: Cli,
    current_directory: &Path,
    logger: &mut Logger<impl Write>,
) -> Result<(), CliError> {
    let started = Instant::now();
    if let Some(command) = cli.command {
        return execute_maintenance(
            command,
            cli.root.as_deref(),
            current_directory,
            logger,
            started,
        );
    }
    let target = cli.target.as_deref().ok_or(CliError::InvalidArguments)?;
    if !cli.check && cli.output.is_none() {
        return Err(CliError::OutputRequired);
    }
    let root_file = resolve_root_file(cli.root.as_deref(), current_directory)?;
    let target = parse_target_id(target)?;
    let mode = if cli.check {
        AnalysisMode::DryRun
    } else {
        AnalysisMode::Build
    };
    let analysis = ProjectAnalyzer::new(&root_file, mode)
        .analyze_with_progress(target.clone(), |module| logger.parsing(module))
        .map_err(CliError::WriteDiagnostics)?;
    for diagnostic in analysis.diagnostics() {
        logger
            .diagnostic(diagnostic)
            .map_err(CliError::WriteDiagnostics)?;
    }
    if analysis.has_errors() {
        return Err(CliError::AnalysisFailed);
    }

    if cli.check {
        logger
            .finished(&format!("checked target `{target}`"), started.elapsed())
            .map_err(CliError::WriteDiagnostics)?;
        return Ok(());
    }

    let project = analysis
        .into_project()
        .ok_or(CliError::ProjectUnavailable)?;
    let plan = Compiler::new()
        .plan(&project, target.clone())
        .map_err(CliError::Compilation)?;
    for stage in plan.stages() {
        for planned_target in stage.targets() {
            logger
                .compiling(planned_target.id())
                .map_err(CliError::WriteDiagnostics)?;
        }
    }
    let markdown = DefaultMarkdownRenderer::new().render(&plan);
    let output = cli.output.ok_or(CliError::OutputRequired)?;
    let output = resolve_from(current_directory, &output);
    if paths_alias(&output, &root_file) {
        return Err(CliError::OutputAliasesRoot(output));
    }
    fs::write(&output, markdown).map_err(|source| CliError::WriteOutput {
        path: output.clone(),
        source,
    })?;
    logger
        .finished(
            &format!("built target `{target}` to `{}`", output.display()),
            started.elapsed(),
        )
        .map_err(CliError::WriteDiagnostics)?;
    Ok(())
}

// 执行文件维护命令并复用项目诊断日志。
fn execute_maintenance(
    command: Command,
    root: Option<&Path>,
    current_directory: &Path,
    logger: &mut Logger<impl Write>,
    started: Instant,
) -> Result<(), CliError> {
    match command {
        Command::Fmt { file, check } => {
            let file = resolve_from(current_directory, &file);
            let source = fs::read_to_string(&file).map_err(|source| CliError::ReadFile {
                path: file.clone(),
                source,
            })?;
            let formatted = match Formatter::new().format(&source) {
                Ok(formatted) => formatted,
                Err(FormatError::ChangedMeaning) => return Err(CliError::FormatChangedMeaning),
                Err(FormatError::Parse(diagnostics)) => {
                    for diagnostic in diagnostics {
                        logger
                            .diagnostic(&crate::project::AnalysisDiagnostic::from_parse(
                                file.clone(),
                                &diagnostic,
                            ))
                            .map_err(CliError::WriteDiagnostics)?;
                    }
                    return Err(CliError::FormatRejected);
                }
            };
            let parsed = MarkfileParser::new(ParseMode::DryRun).parse(ModulePath::root(), &source);
            for diagnostic in parsed.diagnostics() {
                logger
                    .diagnostic(&crate::project::AnalysisDiagnostic::from_parse(
                        file.clone(),
                        diagnostic,
                    ))
                    .map_err(CliError::WriteDiagnostics)?;
            }
            if check && source != formatted {
                return Err(CliError::FormatMismatch(file));
            }
            if !check && source != formatted {
                fs::write(&file, formatted).map_err(|source| CliError::WriteOutput {
                    path: file.clone(),
                    source,
                })?;
            }
            logger
                .finished(
                    &format!(
                        "{} `{}`",
                        if check {
                            "checked formatting of"
                        } else {
                            "formatted"
                        },
                        file.display()
                    ),
                    started.elapsed(),
                )
                .map_err(CliError::WriteDiagnostics)?;
        }
        Command::Lint {
            file: Some(file), ..
        } => {
            let file = resolve_from(current_directory, &file);
            let result = Linter::new()
                .lint_file(&file)
                .map_err(|source| CliError::ReadFile {
                    path: file.clone(),
                    source,
                })?;
            for diagnostic in result.diagnostics() {
                logger
                    .diagnostic(diagnostic)
                    .map_err(CliError::WriteDiagnostics)?;
            }
            if result.has_errors() {
                return Err(CliError::AnalysisFailed);
            }
            logger
                .finished(&format!("linted `{}`", file.display()), started.elapsed())
                .map_err(CliError::WriteDiagnostics)?;
        }
        Command::Lint {
            file: None,
            all: true,
        } => {
            let root_file = resolve_root_file(root, current_directory)?;
            let result = Linter::new()
                .lint_all(&root_file, |module| logger.parsing(module))
                .map_err(CliError::WriteDiagnostics)?;
            for diagnostic in result.diagnostics() {
                logger
                    .diagnostic(diagnostic)
                    .map_err(CliError::WriteDiagnostics)?;
            }
            if result.has_errors() {
                return Err(CliError::AnalysisFailed);
            }
            logger
                .finished("linted project", started.elapsed())
                .map_err(CliError::WriteDiagnostics)?;
        }
        Command::Lint { .. } => return Err(CliError::InvalidArguments),
    }
    Ok(())
}

// 根据显式参数或向上搜索结果确定根 Markfile。
fn resolve_root_file(
    explicit: Option<&Path>,
    current_directory: &Path,
) -> Result<PathBuf, CliError> {
    if let Some(root) = explicit {
        let root = resolve_from(current_directory, root);
        if root.is_dir() {
            return Err(CliError::RootIsDirectory(root));
        }
        return Ok(root);
    }
    find_root_file(current_directory)
        .ok_or_else(|| CliError::RootNotFound(current_directory.to_owned()))
}

// 从起始目录向上寻找最近的 main.mf。
fn find_root_file(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .map(|directory| directory.join("main.mf"))
        .find(|candidate| candidate.is_file())
}

// 将命令行目标转换为稳定的绝对逻辑标识。
fn parse_target_id(value: &str) -> Result<TargetId, CliError> {
    let mut segments = value.split("::").map(str::to_owned).collect::<Vec<_>>();
    let name = segments
        .pop()
        .filter(|name| is_valid_target_name(name))
        .ok_or_else(|| CliError::InvalidTarget(value.to_owned()))?;
    let namespace = if segments.is_empty() {
        ModulePath::root()
    } else {
        ModulePath::new(segments).map_err(|error| invalid_module_path(value, error))?
    };
    Ok(TargetId::new(namespace, name))
}

// 将模块路径错误转换为统一的目标参数错误。
fn invalid_module_path(value: &str, _error: ModulePathError) -> CliError {
    CliError::InvalidTarget(value.to_owned())
}

// 判断目标名称是否可安全用于逻辑标识和 Markdown 标题。
fn is_valid_target_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.chars().any(|character| {
            character.is_whitespace() || matches!(character, '/' | '\\' | ':' | '`')
        })
}

// 将相对路径解释为相对于命令启动目录的路径。
fn resolve_from(current_directory: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        current_directory.join(path)
    }
}

// 尽可能规范化路径后判断两条路径是否指向同一文件。
fn paths_alias(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use std::time::{SystemTime, UNIX_EPOCH};

    // 在唯一临时目录中创建 CLI 测试项目。
    fn fixture(files: &[(&str, &str)]) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = env::temp_dir().join(format!("mkd-cli-{unique}"));
        for (relative, content) in files {
            let file = root.join(relative);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, content).unwrap();
        }
        root
    }

    // 验证 clap 命令定义内部一致。
    #[test]
    fn clap_definition_is_valid() {
        Cli::command().debug_assert();
    }

    // 验证构建参数互斥且维护子命令不接受构建参数。
    #[test]
    fn clap_enforces_output_and_check_constraints() {
        assert!(Cli::try_parse_from(["mkd", "build"]).is_ok());
        assert!(Cli::try_parse_from(["mkd", "build", "--check", "-o", "out.md"]).is_err());
        assert!(Cli::try_parse_from(["mkd", "build", "--check"]).is_ok());
        assert!(Cli::try_parse_from(["mkd", "lint", "file.mf"]).is_ok());
        assert!(Cli::try_parse_from(["mkd", "lint", "--all"]).is_ok());
        assert!(Cli::try_parse_from(["mkd", "lint", "file.mf", "--all"]).is_err());
        assert!(Cli::try_parse_from(["mkd", "fmt", "file.mf", "--check"]).is_ok());
        assert!(Cli::try_parse_from(["mkd", "--check", "fmt", "file.mf"]).is_err());
    }

    // 验证目标参数同时支持根目标和命名空间目标。
    #[test]
    fn target_parser_accepts_root_and_namespaced_targets() {
        assert_eq!(
            parse_target_id("build").unwrap(),
            TargetId::new(ModulePath::root(), "build")
        );
        assert_eq!(
            parse_target_id("catalog::api::publish").unwrap(),
            TargetId::new(ModulePath::parse("catalog::api").unwrap(), "publish")
        );
    }

    // 验证目标参数拒绝空分段和 Markdown 定界符。
    #[test]
    fn target_parser_rejects_invalid_names() {
        for target in ["", "catalog::::build", "catalog::", "bad name", "bad`name"] {
            assert!(parse_target_id(target).is_err(), "{target}");
        }
    }

    // 验证根文件发现选择最近的上级 main.mf。
    #[test]
    fn root_discovery_uses_the_nearest_main_file() {
        let root = fixture(&[
            ("main.mf", ""),
            ("nested/main.mf", ""),
            ("nested/work/.keep", ""),
        ]);

        let found = find_root_file(&root.join("nested/work")).unwrap();

        assert_eq!(found, root.join("nested/main.mf"));
        fs::remove_dir_all(root).unwrap();
    }

    // 验证缺失根文件和目录形式根参数返回明确错误。
    #[test]
    fn root_resolution_reports_missing_and_directory_roots() {
        let root = fixture(&[("nested/.keep", "")]);

        let missing = resolve_root_file(None, &root.join("nested")).unwrap_err();
        let directory = resolve_root_file(Some(Path::new(".")), &root).unwrap_err();

        assert!(matches!(missing, CliError::RootNotFound(_)));
        assert!(matches!(directory, CliError::RootIsDirectory(_)));
        fs::remove_dir_all(root).unwrap();
    }

    // 验证检查模式不生成文件且输出成功信息。
    #[test]
    fn check_mode_analyzes_without_writing_an_artifact() {
        let root = fixture(&[(
            "main.mf",
            "---\n# build\n构建目标。\n- 构建成功。\n---\n> build\n",
        )]);
        let cli = Cli::try_parse_from(["mkd", "build", "--check"]).unwrap();
        let mut logs = Vec::new();
        let mut logger = Logger::new(&mut logs, false);

        execute(cli, &root, &mut logger).unwrap();

        let logs = String::from_utf8(logs).unwrap();
        assert!(logs.contains("Parsing <root>"));
        assert!(logs.contains("Finished checked target `build` in"));
        fs::remove_dir_all(root).unwrap();
    }

    // 验证构建模式写入默认 Markdown 制品。
    #[test]
    fn build_mode_writes_the_compiled_artifact() {
        let root = fixture(&[(
            "main.mf",
            "---\n# build\n构建目标。\n- 构建成功。\n---\n> build\n",
        )]);
        let cli = Cli::try_parse_from(["mkd", "build", "-o", "output.md"]).unwrap();
        let mut logs = Vec::new();
        let mut logger = Logger::new(&mut logs, false);

        execute(cli, &root, &mut logger).unwrap();

        let artifact = fs::read_to_string(root.join("output.md")).unwrap();
        assert!(artifact.starts_with("# 规格实施计划：build\n"));
        let logs = String::from_utf8(logs).unwrap();
        assert!(logs.contains("Compiling build"));
        assert!(logs.contains("Finished built target `build` to"));
        fs::remove_dir_all(root).unwrap();
    }

    // 验证输出文件不能覆盖项目根 Markfile。
    #[test]
    fn build_mode_refuses_to_overwrite_the_root_file() {
        let source = "---\n# build\n构建目标。\n- 构建成功。\n---\n> build\n";
        let root = fixture(&[("main.mf", source)]);
        let cli = Cli::try_parse_from(["mkd", "build", "-o", "main.mf"]).unwrap();
        let mut logs = Vec::new();
        let mut logger = Logger::new(&mut logs, false);

        let error = execute(cli, &root, &mut logger).unwrap_err();

        assert!(matches!(error, CliError::OutputAliasesRoot(_)));
        assert_eq!(fs::read_to_string(root.join("main.mf")).unwrap(), source);
        fs::remove_dir_all(root).unwrap();
    }

    // 验证警告写入标准错误但不阻止检查成功。
    #[test]
    fn warnings_are_rendered_without_failing_the_command() {
        let root = fixture(&[(
            "main.mf",
            ">\n---\n# build\n构建目标。\n- 构建成功。\n---\n> build\n",
        )]);
        let cli = Cli::try_parse_from(["mkd", "build", "--check"]).unwrap();
        let mut logs = Vec::new();
        let mut logger = Logger::new(&mut logs, false);

        execute(cli, &root, &mut logger).unwrap();

        let logs = String::from_utf8(logs).unwrap();
        assert!(logs.contains("Warning "));
        assert!(logs.contains("[W001]"));
        assert!(logs.contains("Finished checked target `build` in"));
        fs::remove_dir_all(root).unwrap();
    }

    // 验证项目错误渲染为带文件位置的诊断。
    #[test]
    fn analysis_errors_are_rendered_to_stderr() {
        let root = fixture(&[(
            "main.mf",
            "---\n# build\n- 规格。\n错误描述。\n---\n> build\n",
        )]);
        let cli = Cli::try_parse_from(["mkd", "build", "--check"]).unwrap();
        let mut logs = Vec::new();
        let mut logger = Logger::new(&mut logs, false);

        let error = execute(cli, &root, &mut logger).unwrap_err();

        assert!(matches!(error, CliError::AnalysisFailed));
        let logs = String::from_utf8(logs).unwrap();
        assert!(logs.contains("Error "));
        assert!(logs.contains("main.mf:4 [E006]"));
        assert!(!logs.contains("Finished"));
        fs::remove_dir_all(root).unwrap();
    }
}
