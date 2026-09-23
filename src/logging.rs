use std::io::{self, Write};
use std::time::Duration;

use clap::ValueEnum;

use crate::core::{ModulePath, TargetId};
use crate::parser::DiagnosticSeverity;
use crate::project::AnalysisDiagnostic;

/// 控制命令行日志是否使用 ANSI 颜色。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
pub enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

impl ColorChoice {
    /// 根据选项和终端状态决定是否着色。
    pub fn enabled(self, is_terminal: bool) -> bool {
        match self {
            Self::Auto => is_terminal,
            Self::Always => true,
            Self::Never => false,
        }
    }
}

/// 将构建进度与诊断统一写为 Cargo 式单行日志。
pub struct Logger<W: Write> {
    writer: W,
    color: bool,
}

impl<W: Write> Logger<W> {
    /// 创建指定颜色模式的日志记录器。
    pub fn new(writer: W, color: bool) -> Self {
        Self { writer, color }
    }

    /// 记录正在解析的模块。
    pub fn parsing(&mut self, module: &ModulePath) -> io::Result<()> {
        let name = if module.is_root() {
            "<root>".to_owned()
        } else {
            module.to_string()
        };
        self.status("Parsing", &name, "\x1b[36m")
    }

    /// 记录正在编译的目标。
    pub fn compiling(&mut self, target: &TargetId) -> io::Result<()> {
        self.status("Compiling", &target.to_string(), "\x1b[36m")
    }

    /// 将文件级诊断记录为单行错误或警告。
    pub fn diagnostic(&mut self, diagnostic: &AnalysisDiagnostic) -> io::Result<()> {
        let location = match diagnostic.line() {
            Some(line) => format!("{}:{line}", diagnostic.file().display()),
            None => diagnostic.file().display().to_string(),
        };
        let message = format!(
            "{location} [{}] {}",
            diagnostic.code(),
            diagnostic.message()
        );
        match diagnostic.severity() {
            DiagnosticSeverity::Error => self.error(&message),
            DiagnosticSeverity::Warning => self.warning(&message),
        }
    }

    /// 记录单行错误。
    pub fn error(&mut self, message: &str) -> io::Result<()> {
        self.status("Error", message, "\x1b[31m")
    }

    /// 记录单行警告。
    pub fn warning(&mut self, message: &str) -> io::Result<()> {
        self.status("Warning", message, "\x1b[33m")
    }

    /// 记录检查或构建的完成状态和耗时。
    pub fn finished(&mut self, message: &str, elapsed: Duration) -> io::Result<()> {
        self.status(
            "Finished",
            &format!("{message} in {:.2}s", elapsed.as_secs_f64()),
            "\x1b[1;36m",
        )
    }

    // 对齐状态标签并在需要时仅为标签添加颜色。
    fn status(&mut self, label: &str, message: &str, style: &str) -> io::Result<()> {
        if self.color {
            writeln!(
                self.writer,
                "{style}{label:>12}\x1b[0m {}",
                single_line(message)
            )
        } else {
            writeln!(self.writer, "{label:>12} {}", single_line(message))
        }
    }
}

// 将诊断文本规范为不破坏日志行结构的单行内容。
fn single_line(message: &str) -> String {
    message
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .trim_end()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    // 验证自动颜色只在终端中启用。
    #[test]
    fn auto_color_follows_terminal_detection() {
        assert!(!ColorChoice::Auto.enabled(false));
        assert!(ColorChoice::Auto.enabled(true));
        assert!(ColorChoice::Always.enabled(false));
        assert!(!ColorChoice::Never.enabled(true));
    }

    // 验证纯文本进度对齐并保持单行。
    #[test]
    fn plain_logs_align_labels_and_escape_newlines() {
        let mut output = Vec::new();
        let mut logger = Logger::new(&mut output, false);
        logger.parsing(&ModulePath::root()).unwrap();
        logger.error("first\nsecond\x1b").unwrap();
        logger
            .finished("checked target `build`", Duration::from_millis(15))
            .unwrap();

        assert_eq!(
            String::from_utf8(output).unwrap(),
            "     Parsing <root>\n       Error first second\n    Finished checked target `build` in 0.01s\n"
        );
    }

    // 验证着色只影响标签且结束标签为粗体青色。
    #[test]
    fn colored_logs_use_cyan_progress_and_red_errors() {
        let mut output = Vec::new();
        let mut logger = Logger::new(&mut output, true);
        logger
            .compiling(&TargetId::new(ModulePath::root(), "build"))
            .unwrap();
        logger.error("failed").unwrap();
        logger
            .finished("built target `build`", Duration::ZERO)
            .unwrap();

        assert_eq!(
            String::from_utf8(output).unwrap(),
            "\x1b[36m   Compiling\x1b[0m build\n\x1b[31m       Error\x1b[0m failed\n\x1b[1;36m    Finished\x1b[0m built target `build` in 0.00s\n"
        );
    }
}
