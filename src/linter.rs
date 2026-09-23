use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::core::ModulePath;
use crate::parser::{DiagnosticSeverity, ParseMode, ParsedModule, Parser};
use crate::project::{AnalysisDiagnostic, AnalysisMode, ProjectAnalyzer};

/// 汇总文件或项目的静态诊断。
#[derive(Debug)]
pub struct LintResult {
    diagnostics: Vec<AnalysisDiagnostic>,
}

impl LintResult {
    /// 返回所有语法、语义与质量诊断。
    pub fn diagnostics(&self) -> &[AnalysisDiagnostic] {
        &self.diagnostics
    }

    /// 判断是否存在阻塞性错误。
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|item| item.severity() == DiagnosticSeverity::Error)
    }
}

/// 对 Markfile 执行不修改源码的静态检查。
#[derive(Default)]
pub struct Linter;

impl Linter {
    /// 创建静态检查器。
    pub fn new() -> Self {
        Self
    }

    /// 检查单文件语法与质量规则，不解析项目引用。
    pub fn lint_file(&self, file: &Path) -> io::Result<LintResult> {
        let source = fs::read_to_string(file)?;
        Ok(self.lint_source(file, &source))
    }

    /// 检查编辑器中的源码文本而不读取磁盘文件。
    pub fn lint_source(&self, file: &Path, source: &str) -> LintResult {
        let parsed = Parser::new(ParseMode::DryRun).parse(ModulePath::root(), source);
        let mut diagnostics = parsed
            .diagnostics()
            .iter()
            .map(|item| AnalysisDiagnostic::from_parse(file.to_path_buf(), item))
            .collect::<Vec<_>>();
        lint_module(file, parsed.partial_module(), &mut diagnostics);
        LintResult { diagnostics }
    }

    /// 检查根目录内所有 Markfile 及跨模块引用和依赖环。
    pub fn lint_all(
        &self,
        root_file: &Path,
        progress: impl FnMut(&ModulePath) -> io::Result<()>,
    ) -> io::Result<LintResult> {
        let mut quality = Vec::new();
        let analysis = ProjectAnalyzer::new(root_file, AnalysisMode::DryRun)
            .analyze_all_with_modules(progress, |file, module| {
                lint_module(file, module, &mut quality)
            })?;
        let mut diagnostics = analysis.diagnostics().to_vec();
        diagnostics.extend(quality);
        Ok(LintResult { diagnostics })
    }
}

// 检查目标文本与导入的使用情况，沿用解析器的一基行号。
fn lint_module(file: &Path, module: &ParsedModule, diagnostics: &mut Vec<AnalysisDiagnostic>) {
    for target in module.targets() {
        if target.description().trim().is_empty() {
            diagnostics.push(warning(
                file,
                target.line(),
                "L001",
                "target has no description",
            ));
        }
        if target.specifications().is_empty() {
            diagnostics.push(warning(
                file,
                target.line(),
                "L002",
                "target has no specifications",
            ));
        }
    }
    let local_names = module
        .targets()
        .iter()
        .map(|target| target.name())
        .collect::<BTreeSet<_>>();
    let used = module
        .targets()
        .iter()
        .flat_map(|target| target.dependencies())
        .filter(|dep| dep.path().segments().len() == 1)
        .map(|dep| dep.path().segments()[0].as_str())
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeMap::new();
    for include in module.includes() {
        if include.target().path().is_empty() {
            continue;
        }
        let alias = include.alias().or_else(|| {
            include
                .target()
                .path()
                .segments()
                .last()
                .map(String::as_str)
        });
        let Some(alias) = alias else {
            continue;
        };
        let key = (
            include.target().path().segments().to_vec(),
            alias.to_owned(),
        );
        if seen.insert(key, include.target().line()).is_some() {
            diagnostics.push(warning(
                file,
                include.target().line(),
                "L004",
                "duplicate import",
            ));
        }
        if !used.contains(alias) || local_names.contains(alias) {
            diagnostics.push(warning(
                file,
                include.target().line(),
                "L003",
                format!("unused import `{alias}`"),
            ));
        }
    }
}

// 为文件质量规则创建非阻塞诊断。
fn warning(
    file: &Path,
    line: usize,
    code: &'static str,
    message: impl Into<String>,
) -> AnalysisDiagnostic {
    AnalysisDiagnostic::warning(PathBuf::from(file), line, code, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    // 验证全项目检查覆盖未引用的模块及跨模块错误。
    #[test]
    fn all_scans_unreachable_modules_and_checks_references() {
        let root = std::env::temp_dir().join(format!(
            "mkd-lint-all-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("main.mf"),
            "---\n# start\n- ready\n---\n> start\n",
        )
        .unwrap();
        fs::write(
            root.join("orphan.mf"),
            "> crate::absent::missing\n---\n# unused\n---\n> unused\n",
        )
        .unwrap();
        let mut visited = Vec::new();

        let result = Linter::new()
            .lint_all(&root.join("main.mf"), |module| {
                visited.push(module.clone());
                Ok(())
            })
            .unwrap();

        assert!(visited.contains(&ModulePath::parse("orphan").unwrap()));
        assert!(
            result
                .diagnostics()
                .iter()
                .any(|item| item.code() == "P001")
        );
        assert!(result.has_errors());
        fs::remove_dir_all(root).unwrap();
    }

    // 验证全量扫描也报告未引用文件中的语法错误和依赖环。
    #[test]
    fn all_detects_unreachable_syntax_errors_and_cycles() {
        let root = std::env::temp_dir().join(format!(
            "mkd-lint-graph-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("main.mf"),
            "---\n# start\n- ready\n---\n> start\n",
        )
        .unwrap();
        fs::write(root.join("cycle.mf"), "---\n# a\n> b\n# b\n> a\n---\n> a\n").unwrap();
        fs::write(root.join("broken.mf"), "not a directive\n---\n---\n").unwrap();

        let result = Linter::new()
            .lint_all(&root.join("main.mf"), |_| Ok(()))
            .unwrap();
        assert!(result.has_errors());
        assert!(
            result
                .diagnostics()
                .iter()
                .any(|item| item.code() == "E002" && item.file().ends_with("broken.mf"))
        );
        assert!(
            result
                .diagnostics()
                .iter()
                .any(|item| item.code() == "P014")
        );
        fs::remove_dir_all(root).unwrap();
    }

    // 验证符号链接指向的独立模块同样参与全量扫描。
    #[cfg(unix)]
    #[test]
    fn all_scans_linked_markfiles() {
        use std::os::unix::fs::symlink;
        let root = std::env::temp_dir().join(format!(
            "mkd-lint-links-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("main.mf"),
            "---\n# start\n- ready\n---\n> start\n",
        )
        .unwrap();
        fs::write(
            root.join("source.txt"),
            "---\n# linked\n> missing\n---\n> linked\n",
        )
        .unwrap();
        symlink(root.join("source.txt"), root.join("linked.mf")).unwrap();

        let result = Linter::new()
            .lint_all(&root.join("main.mf"), |_| Ok(()))
            .unwrap();
        assert!(
            result
                .diagnostics()
                .iter()
                .any(|item| item.code() == "P005" && item.file().ends_with("linked.mf"))
        );
        fs::remove_dir_all(root).unwrap();
    }

    // 验证文件检查报告质量警告但不阻塞退出。
    #[test]
    fn reports_file_warnings_without_errors() {
        let file = std::env::temp_dir().join(format!(
            "mkd-lint-{}.mf",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(
            &file,
            "> crate::unused::helper\n> crate::unused::helper\n---\n# build\n---\n> build\n",
        )
        .unwrap();
        let result = Linter::new().lint_file(&file).unwrap();
        let codes = result
            .diagnostics()
            .iter()
            .map(AnalysisDiagnostic::code)
            .collect::<Vec<_>>();
        assert!(codes.contains(&"L001"));
        assert!(codes.contains(&"L002"));
        assert!(codes.contains(&"L003"));
        assert!(codes.contains(&"L004"));
        assert!(!result.has_errors());
        fs::remove_file(file).unwrap();
    }
}
