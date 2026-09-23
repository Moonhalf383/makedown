use crate::core::ModulePath;
use crate::parser::{Diagnostic, ParseMode, ParsedModule, Parser};

/// 表示无法安全格式化的输入。
#[derive(Debug, Eq, PartialEq)]
pub enum FormatError {
    /// 输入包含阻塞性解析诊断。
    Parse(Vec<Diagnostic>),
    /// 规范化会改变解析后的规格内容。
    ChangedMeaning,
}

/// 对完整 Markfile 源码执行保守且幂等的格式化。
#[derive(Default)]
pub struct Formatter;

impl Formatter {
    /// 创建文件格式化器。
    pub fn new() -> Self {
        Self
    }

    /// 格式化源码；解析失败或语义变化时不返回可写入内容。
    pub fn format(&self, source: &str) -> Result<String, FormatError> {
        let parser = Parser::new(ParseMode::Build);
        let original = parser.parse(ModulePath::root(), source);
        if original.has_errors() {
            return Err(FormatError::Parse(original.diagnostics().to_vec()));
        }
        let style = source
            .split_inclusive('\n')
            .find(|line| !line.trim().is_empty())
            .map(|line| if line.ends_with("\r\n") { "\r\n" } else { "\n" })
            .unwrap_or("\n");
        let mut section = 0;
        let mut lines = Vec::<String>::new();
        let mut pending_blanks = Vec::<String>::new();
        for raw in source.lines() {
            let line = raw.trim_end();
            if line.trim().is_empty() {
                pending_blanks.push(String::new());
                continue;
            }
            let separator = line == "---";
            let title = section == 1 && line.starts_with('#');
            let boundary = separator || title;
            if boundary {
                if !lines.is_empty() && !lines.last().is_some_and(String::is_empty) {
                    lines.push(String::new());
                }
            } else if lines.last().is_some_and(|previous| previous == "---") {
                lines.push(String::new());
            } else {
                lines.append(&mut pending_blanks);
            }
            pending_blanks.clear();
            let normalized = if title {
                format!("# {}", line.trim_start_matches('#').trim())
            } else if let Some(value) = line.strip_prefix('>')
                && (value.is_empty() || value.starts_with(char::is_whitespace))
            {
                let value = value.trim();
                if value.is_empty() {
                    ">".to_owned()
                } else {
                    format!("> {value}")
                }
            } else if section == 1
                && let Some(value) = line.strip_prefix('-')
                && (value.is_empty() || value.starts_with(char::is_whitespace))
            {
                let value = value.trim();
                if value.is_empty() {
                    "-".to_owned()
                } else {
                    format!("- {value}")
                }
            } else {
                line.to_owned()
            };
            lines.push(normalized);
            if separator {
                section += 1;
            }
        }
        let result = format!("{}{}", lines.join(style), style);
        let formatted = parser.parse(ModulePath::root(), &result);
        if formatted.has_errors()
            || !equivalent(original.partial_module(), formatted.partial_module())
        {
            return Err(FormatError::ChangedMeaning);
        }
        Ok(result)
    }
}

// 对比忽略行号的语法内容，允许结构边界插入空行。
fn equivalent(before: &ParsedModule, after: &ParsedModule) -> bool {
    before
        .includes()
        .iter()
        .map(|item| (item.target().path(), item.alias()))
        .eq(after
            .includes()
            .iter()
            .map(|item| (item.target().path(), item.alias())))
        && before
            .targets()
            .iter()
            .map(|item| {
                (
                    item.name(),
                    item.dependencies()
                        .iter()
                        .map(|dep| dep.path())
                        .collect::<Vec<_>>(),
                    item.description(),
                    item.specifications(),
                )
            })
            .eq(after.targets().iter().map(|item| {
                (
                    item.name(),
                    item.dependencies()
                        .iter()
                        .map(|dep| dep.path())
                        .collect::<Vec<_>>(),
                    item.description(),
                    item.specifications(),
                )
            }))
        && before
            .public_targets()
            .iter()
            .map(|item| item.name())
            .eq(after.public_targets().iter().map(|item| item.name()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // 验证结构规范化、描述缩进保留及重复格式化的幂等性。
    #[test]
    fn formats_without_changing_meaning() {
        let source = ">  crate::other::x  \n---\n##  run  \n  # not a target\n\nparagraph  \n>  x \n-  spec  \n---\n>  run \n";
        let formatted = Formatter::new().format(source).unwrap();
        assert!(formatted.contains("# run\n  # not a target\n\nparagraph\n> x\n- spec"));
        assert_eq!(Formatter::new().format(&formatted).unwrap(), formatted);
    }

    // 验证结构边界固定空行且内部段落和依赖顺序保持不变。
    #[test]
    fn normalizes_boundaries_but_keeps_description_paragraphs() {
        let source = "> crate::a::x\n> crate::a::y\n\n\n---\n\n\n# first\nfirst paragraph\n\nsecond paragraph\n\n\n# second\n> x\n> y\n- done\n---\n> second\n";
        let formatted = Formatter::new().format(source).unwrap();
        assert!(formatted.contains("> crate::a::y\n\n---\n\n# first"));
        assert!(formatted.contains("first paragraph\n\nsecond paragraph\n\n# second"));
        assert!(formatted.contains("# second\n> x\n> y\n- done\n\n---"));
        assert_eq!(Formatter::new().format(&formatted).unwrap(), formatted);
    }

    // 验证空指令保持原样并不删除解析器发出的警告。
    #[test]
    fn keeps_empty_directives() {
        let source = ">  \n---\n# build\n>   \n- spec\n---\n>  \n";
        let formatted = Formatter::new().format(source).unwrap();
        assert_eq!(formatted.matches("\n>\n").count(), 2);
        let parsed = Parser::new(ParseMode::DryRun).parse(ModulePath::root(), &formatted);
        assert_eq!(
            parsed
                .diagnostics()
                .iter()
                .filter(|item| item.code() == "W001")
                .count(),
            3
        );
        assert_eq!(Formatter::new().format(&formatted).unwrap(), formatted);
    }

    // 验证有错误时拒绝修改，且保留原有 CRLF 风格。
    #[test]
    fn rejects_errors_and_preserves_crlf() {
        assert!(matches!(
            Formatter::new().format("broken"),
            Err(FormatError::Parse(_))
        ));
        let formatted = Formatter::new().format("---\r\n# x\r\n---\r\n> x").unwrap();
        assert!(formatted.ends_with("\r\n"));
        assert!(!formatted.replace("\r\n", "").contains('\n'));
    }
}
