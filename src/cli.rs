use std::env;
use std::error::Error;
use std::fmt;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use clap::{Parser, Subcommand};

use crate::compiler::{CompileError, Compiler, DefaultMarkdownRenderer, TemplateMarkdownRenderer};
use crate::config::{ConfigError, ProjectConfig, project_path};
use crate::core::{ModulePath, ModulePathError, Project, TargetId, TargetName};
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
    about = "Analyze and compile Markfile specifications"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    #[arg(
        long,
        value_name = "MARKFILE",
        global = true,
        help = "Use MARKFILE as the project root instead of searching for main.mf"
    )]
    root: Option<PathBuf>,

    #[arg(long, value_enum, default_value_t = ColorChoice::Auto, global = true, help = "Control ANSI colors in progress and diagnostic logs")]
    color: ColorChoice,
}

// 定义独立于目标构建的维护命令。
#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "Compile a Target directly without reading mkd.toml")]
    Target {
        #[arg(
            value_name = "TARGET",
            help = "Target to check or compile (target or module::target)"
        )]
        target: String,
        #[arg(
            short,
            long,
            value_name = "FILE",
            required_unless_present = "check",
            conflicts_with = "check",
            help = "Write the compiled Markdown artifact to FILE"
        )]
        output: Option<PathBuf>,
        #[arg(
            long,
            value_name = "FILE",
            conflicts_with = "no_template",
            help = "Render the plan with a Markdown template"
        )]
        template: Option<PathBuf>,
        #[arg(
            long,
            conflicts_with = "template",
            help = "Use the default Markdown renderer"
        )]
        no_template: bool,
        #[arg(long, help = "Analyze the project without writing an artifact")]
        check: bool,
    },
    #[command(about = "Build a named project profile (default if omitted)")]
    Build {
        #[arg(
            value_name = "PROFILE",
            help = "Named build profile; defaults to [build].default"
        )]
        name: Option<String>,
        #[arg(
            short,
            long,
            value_name = "FILE",
            conflicts_with = "check",
            help = "Override the configured output path"
        )]
        output: Option<PathBuf>,
        #[arg(
            long,
            value_name = "FILE",
            conflicts_with = "no_template",
            help = "Override the configured Markdown template"
        )]
        template: Option<PathBuf>,
        #[arg(
            long,
            conflicts_with = "template",
            help = "Ignore the configured template"
        )]
        no_template: bool,
        #[arg(
            long,
            help = "Validate the profile and effective template without writing an artifact"
        )]
        check: bool,
    },
    #[command(about = "Initialize main.mf and mkd.toml")]
    Init {
        #[arg(value_name = "DIRECTORY")]
        directory: Option<PathBuf>,
        #[arg(long, value_name = "TARGET")]
        target: Option<String>,
    },
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
    OutputAliasesInput(PathBuf),
    Compilation(CompileError),
    Config(ConfigError),
    ProfileUnavailable(String),
    Init(String),
    Template {
        path: PathBuf,
        line: Option<usize>,
        column: Option<usize>,
        source: minijinja::Error,
    },
    InvalidArguments,
    FormatRejected,
    FormatChangedMeaning,
    FormatMismatch(PathBuf),
    ReadFile {
        path: PathBuf,
        source: io::Error,
    },
    WriteOutput {
        path: PathBuf,
        source: io::Error,
    },
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
            Self::OutputAliasesInput(path) => write!(
                formatter,
                "output path `{}` would overwrite a template or project Markfile",
                path.display()
            ),
            Self::Compilation(error) => write!(formatter, "compilation failed: {error}"),
            Self::Config(error) => write!(formatter, "{error}"),
            Self::ProfileUnavailable(message) | Self::Init(message) => formatter.write_str(message),
            Self::Template {
                path,
                line,
                column,
                source,
            } => {
                write!(formatter, "template `{}`", path.display())?;
                if let Some(line) = line {
                    write!(formatter, ":{line}")?;
                    if let Some(column) = column {
                        write!(formatter, ":{column}")?;
                    }
                }
                write!(formatter, ": {}", source.kind())?;
                if let Some(detail) = source.detail() {
                    write!(formatter, ": {detail}")?;
                }
                Ok(())
            }
            Self::InvalidArguments => formatter.write_str("invalid command arguments"),
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
    match cli.command {
        Command::Init { directory, target } => {
            if cli.root.is_some() {
                return Err(CliError::Init("--root cannot be used with init".into()));
            }
            execute_init(
                directory.as_deref(),
                target.as_deref(),
                current_directory,
                logger,
                started,
            )
        }
        Command::Build {
            name,
            output,
            template,
            no_template,
            check,
        } => {
            let root_file = resolve_root_file(cli.root.as_deref(), current_directory)?;
            let config = ProjectConfig::load(&root_file)
                .map_err(CliError::Config)?
                .ok_or_else(|| {
                    CliError::ProfileUnavailable(format!(
                        "no mkd.toml beside `{}`; use `mkd init` or `mkd target TARGET`",
                        root_file.display()
                    ))
                })?;
            let (_profile_name, profile) = config.profile(name.as_deref()).ok_or_else(|| {
                CliError::ProfileUnavailable(format!(
                    "unknown or missing build profile `{}`",
                    name.as_deref().unwrap_or("<default>")
                ))
            })?;
            let target = profile.target_id().map_err(CliError::Init)?;
            let project_dir = root_file.parent().unwrap_or_else(|| Path::new("."));
            let output = match output {
                Some(path) => resolve_from(current_directory, &path),
                None => project_path(project_dir, profile.output())
                    .map_err(CliError::ProfileUnavailable)?,
            };
            let template_path = if no_template {
                None
            } else if let Some(path) = template {
                Some(resolve_from(current_directory, &path))
            } else {
                profile
                    .template()
                    .map(|path| project_path(project_dir, path))
                    .transpose()
                    .map_err(CliError::ProfileUnavailable)?
            };
            compile_target(
                &root_file,
                target,
                Some(output),
                template_path,
                check,
                logger,
                started,
            )
        }
        Command::Target {
            target,
            output,
            template,
            no_template,
            check,
        } => {
            let root_file = resolve_root_file(cli.root.as_deref(), current_directory)?;
            let target = parse_target_id(&target)?;
            let output = output.map(|path| resolve_from(current_directory, &path));
            let template_path = if no_template {
                None
            } else {
                template
                    .as_deref()
                    .map(|path| resolve_from(current_directory, path))
            };
            compile_target(
                &root_file,
                target,
                output,
                template_path,
                check,
                logger,
                started,
            )
        }
        command => execute_maintenance(
            command,
            cli.root.as_deref(),
            current_directory,
            logger,
            started,
        ),
    }
}

// 分析指定目标并按可选模板检查或写出 Markdown。
fn compile_target(
    root_file: &Path,
    target: TargetId,
    output: Option<PathBuf>,
    template_path: Option<PathBuf>,
    check: bool,
    logger: &mut Logger<impl Write>,
    started: Instant,
) -> Result<(), CliError> {
    let mode = if check && template_path.is_none() {
        AnalysisMode::DryRun
    } else {
        AnalysisMode::Build
    };
    let analysis = ProjectAnalyzer::new(root_file, mode)
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

    if check && template_path.is_none() {
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
    if !check {
        for stage in plan.stages() {
            for planned_target in stage.targets() {
                logger
                    .compiling(planned_target.id())
                    .map_err(CliError::WriteDiagnostics)?;
            }
        }
    }
    let markdown = if let Some(path) = &template_path {
        let source = fs::read_to_string(path).map_err(|source| CliError::ReadFile {
            path: path.clone(),
            source,
        })?;
        TemplateMarkdownRenderer::new()
            .render(&source, &plan)
            .map_err(|error| template_error(path, &source, error))?
    } else {
        DefaultMarkdownRenderer::new().render(&plan)
    };
    if check {
        logger
            .finished(
                &format!("checked target `{target}` and template"),
                started.elapsed(),
            )
            .map_err(CliError::WriteDiagnostics)?;
        return Ok(());
    }
    let output = output.ok_or(CliError::OutputRequired)?;
    // 缺失的父目录会使 canonicalize 失败，先建目录再判断真实路径别名。
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|source| CliError::WriteOutput {
            path: output.clone(),
            source,
        })?;
    }
    if paths_alias(&output, root_file) {
        return Err(CliError::OutputAliasesRoot(output));
    }
    if template_path
        .as_ref()
        .is_some_and(|path| paths_alias(&output, path))
        || project_input_files(root_file, &project)
            .iter()
            .any(|path| paths_alias(&output, path))
        || paths_alias(&output, &root_file.with_file_name("mkd.toml"))
    {
        return Err(CliError::OutputAliasesInput(output));
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

// 根据目录中已有文件安全初始化可构建项目。
fn execute_init(
    directory: Option<&Path>,
    target: Option<&str>,
    current_directory: &Path,
    logger: &mut Logger<impl Write>,
    started: Instant,
) -> Result<(), CliError> {
    let directory = directory
        .map(|path| resolve_from(current_directory, path))
        .unwrap_or_else(|| current_directory.to_path_buf());
    let root_file = directory.join("main.mf");
    let config_file = directory.join("mkd.toml");
    let has_root = root_file.exists();
    let has_config = config_file.exists();
    if has_config {
        return Err(CliError::Init(format!(
            "`{}` already exists; init will not overwrite existing configuration",
            config_file.display()
        )));
    }
    let target = match (has_root, target) {
        (true, None) => {
            return Err(CliError::Init(
                "main.mf already exists; pass --target TARGET to create its build profile".into(),
            ));
        }
        (true, Some(name)) => {
            let target = parse_target_id(name)?;
            let analysis =
                ProjectAnalyzer::new(&root_file, AnalysisMode::Build).analyze(target.clone());
            if analysis.has_errors() {
                return Err(CliError::Init(format!(
                    "cannot initialize: target `{target}` in `{}` is not buildable: {}",
                    root_file.display(),
                    analysis
                        .diagnostics()
                        .iter()
                        .filter(|item| item.severity() == crate::parser::DiagnosticSeverity::Error)
                        .map(|item| item.message().to_owned())
                        .collect::<Vec<_>>()
                        .join("; ")
                )));
            }
            target
        }
        (false, Some(name)) => {
            let target = parse_target_id(name)?;
            if !target.namespace().is_root() {
                return Err(CliError::Init(
                    "new projects require a local --target name".into(),
                ));
            }
            target
        }
        (false, None) => TargetId::new(
            ModulePath::root(),
            TargetName::parse("start").expect("static target name"),
        ),
    };
    let config = format!(
        "version = 1\n\n[build]\ndefault = \"main\"\n\n[build.main]\ntarget = {}\noutput = \"dist/plan.md\"\n",
        toml::Value::String(target.to_string())
    );
    let markfile = format!(
        "---\n# {}\n描述要完成的工作。\n- 明确一条可核验的验收规格。\n---\n> {}\n",
        target.name(),
        target.name()
    );
    fs::create_dir_all(&directory).map_err(|source| CliError::WriteOutput {
        path: directory.clone(),
        source,
    })?;
    if !has_root {
        use std::io::Write as _;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&root_file)
            .map_err(|source| CliError::WriteOutput {
                path: root_file.clone(),
                source,
            })?;
        if let Err(source) = file.write_all(markfile.as_bytes()) {
            let _ = fs::remove_file(&root_file);
            return Err(CliError::WriteOutput {
                path: root_file,
                source,
            });
        }
    }
    let result = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&config_file);
    let mut file = match result {
        Ok(file) => file,
        Err(source) => {
            if !has_root {
                let _ = fs::remove_file(&root_file);
            }
            return Err(CliError::WriteOutput {
                path: config_file,
                source,
            });
        }
    };
    if let Err(source) = file.write_all(config.as_bytes()) {
        let _ = fs::remove_file(&config_file);
        if !has_root {
            let _ = fs::remove_file(&root_file);
        }
        return Err(CliError::WriteOutput {
            path: config_file,
            source,
        });
    }
    logger
        .finished(
            &format!("initialized `{}`", directory.display()),
            started.elapsed(),
        )
        .map_err(CliError::WriteDiagnostics)?;
    Ok(())
}

// 将模板错误映射为输入文件中的行列位置。
fn template_error(path: &Path, source: &str, error: minijinja::Error) -> CliError {
    let column = error.range().and_then(|range| {
        let prefix = source.get(..range.start)?;
        Some(prefix.rsplit('\n').next()?.chars().count() + 1)
    });
    CliError::Template {
        path: path.to_path_buf(),
        line: error.line(),
        column,
        source: error,
    }
}

// 枚举分析生成的项目所包含的所有 Markfile 输入路径。
fn project_input_files(root_file: &Path, project: &Project) -> Vec<PathBuf> {
    project
        .modules()
        .map(|module| {
            if module.namespace().is_root() {
                return root_file.to_path_buf();
            }
            let mut path = root_file
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .to_path_buf();
            let segments = module.namespace().segments();
            for segment in &segments[..segments.len() - 1] {
                path.push(segment);
            }
            path.push(format!("{}.mf", segments.last().unwrap()));
            path
        })
        .collect()
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
        Command::Lint { .. }
        | Command::Target { .. }
        | Command::Build { .. }
        | Command::Init { .. } => {
            return Err(CliError::InvalidArguments);
        }
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
        .ok_or_else(|| CliError::InvalidTarget(value.to_owned()))?;
    let name = TargetName::parse(&name).map_err(|_| CliError::InvalidTarget(value.to_owned()))?;
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
    if left == right {
        return true;
    }
    if let (Ok(left), Ok(right)) = (left.canonicalize(), right.canonicalize())
        && left == right
    {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let (Ok(left), Ok(right)) = (fs::metadata(left), fs::metadata(right)) {
            return left.dev() == right.dev() && left.ino() == right.ino();
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::TargetName;
    use clap::CommandFactory;
    use std::time::{SystemTime, UNIX_EPOCH};

    // 为测试快速构造经过校验的目标标识。
    fn id(namespace: ModulePath, name: &str) -> TargetId {
        TargetId::new(namespace, TargetName::parse(name).unwrap())
    }

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

    // 验证两种构建入口互不歧义且维护子命令不接受构建参数。
    #[test]
    fn clap_enforces_output_and_check_constraints() {
        assert!(Cli::try_parse_from(["mkd", "build"]).is_ok());
        assert!(Cli::try_parse_from(["mkd", "target", "build"]).is_err());
        assert!(Cli::try_parse_from(["mkd", "target", "build", "--check"]).is_ok());
        assert!(Cli::try_parse_from(["mkd", "target", "build", "-o", "out.md"]).is_ok());
        assert!(
            Cli::try_parse_from(["mkd", "target", "build", "--check", "-o", "out.md"]).is_err()
        );
        assert!(Cli::try_parse_from(["mkd", "target", "profile", "--check"]).is_ok());
        assert!(Cli::try_parse_from(["mkd", "build", "--check", "-o", "out.md"]).is_err());
        assert!(Cli::try_parse_from(["mkd", "build", "--check"]).is_ok());
        assert!(Cli::try_parse_from(["mkd", "profile"]).is_err());
        assert!(Cli::try_parse_from(["mkd", "greet", "--check"]).is_err());
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
            id(ModulePath::root(), "build")
        );
        assert_eq!(
            parse_target_id("catalog::api::publish").unwrap(),
            id(ModulePath::parse("catalog::api").unwrap(), "publish")
        );
    }

    // 验证目标参数拒绝空分段和 Markdown 定界符。
    #[test]
    fn target_parser_rejects_invalid_names() {
        for target in [
            "",
            "catalog::::build",
            "catalog::",
            "bad name",
            "bad`name",
            "bad/name",
            "bad\tname",
            "catalog::.",
        ] {
            assert!(parse_target_id(target).is_err(), "{target}");
        }
        assert_eq!(
            parse_target_id("😀build").unwrap(),
            id(ModulePath::root(), "😀build")
        );
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
        let cli = Cli::try_parse_from(["mkd", "target", "build", "--check"]).unwrap();
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
        let cli = Cli::try_parse_from(["mkd", "target", "build", "-o", "output.md"]).unwrap();
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
        let cli = Cli::try_parse_from(["mkd", "target", "build", "-o", "main.mf"]).unwrap();
        let mut logs = Vec::new();
        let mut logger = Logger::new(&mut logs, false);

        let error = execute(cli, &root, &mut logger).unwrap_err();

        assert!(matches!(error, CliError::OutputAliasesRoot(_)));
        assert_eq!(fs::read_to_string(root.join("main.mf")).unwrap(), source);
        fs::remove_dir_all(root).unwrap();
    }

    // 验证保护输入文件时能识别与模板或 Markfile 共用 inode 的硬链接。
    #[cfg(unix)]
    #[test]
    fn paths_alias_recognizes_hard_links() {
        let root = fixture(&[("main.mf", "---\n# build\n---\n> build\n")]);
        std::fs::hard_link(root.join("main.mf"), root.join("artifact.md")).unwrap();

        assert!(paths_alias(
            &root.join("artifact.md"),
            &root.join("main.mf")
        ));
        fs::remove_dir_all(root).unwrap();
    }

    // 验证警告写入标准错误但不阻止检查成功。
    #[test]
    fn warnings_are_rendered_without_failing_the_command() {
        let root = fixture(&[(
            "main.mf",
            ">\n---\n# build\n构建目标。\n- 构建成功。\n---\n> build\n",
        )]);
        let cli = Cli::try_parse_from(["mkd", "target", "build", "--check"]).unwrap();
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
        let cli = Cli::try_parse_from(["mkd", "target", "build", "--check"]).unwrap();
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
