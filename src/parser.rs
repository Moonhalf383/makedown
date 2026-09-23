use std::collections::BTreeSet;

use crate::core::ModulePath;

/// 保存尚未由项目分析器解析的目标路径。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetPath {
    segments: Vec<String>,
}

impl TargetPath {
    /// 从原始路径分段创建未解析路径。
    pub fn new(segments: Vec<String>) -> Self {
        Self { segments }
    }

    /// 返回未解析路径的分段。
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// 判断路径是否来自空指令。
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }
}

/// 记录未解析目标路径及其源码行号。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedReference {
    path: TargetPath,
    line: usize,
}

impl ParsedReference {
    // 创建带源码位置的未解析引用。
    fn new(path: TargetPath, line: usize) -> Self {
        Self { path, line }
    }

    /// 返回引用中的未解析路径。
    pub fn path(&self) -> &TargetPath {
        &self.path
    }

    /// 返回引用所在的一基行号。
    pub fn line(&self) -> usize {
        self.line
    }
}

/// 表示文件头部的一项目标导入。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedInclude {
    target: ParsedReference,
    alias: Option<String>,
}

impl ParsedInclude {
    /// 返回导入指向的目标引用。
    pub fn target(&self) -> &ParsedReference {
        &self.target
    }

    /// 返回显式声明的本地别名。
    pub fn alias(&self) -> Option<&str> {
        self.alias.as_deref()
    }
}

/// 表示单文件语法层中的目标。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedTarget {
    name: String,
    line: usize,
    dependencies: Vec<ParsedReference>,
    description: String,
    specifications: Vec<String>,
}

impl ParsedTarget {
    /// 返回目标声明的本地名称。
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回目标声明所在的一基行号。
    pub fn line(&self) -> usize {
        self.line
    }

    /// 返回目标尚未解析的依赖引用。
    pub fn dependencies(&self) -> &[ParsedReference] {
        &self.dependencies
    }

    /// 返回目标的描述文本。
    pub fn description(&self) -> &str {
        &self.description
    }

    /// 返回目标的原始规格文本。
    pub fn specifications(&self) -> &[String] {
        &self.specifications
    }
}

/// 记录公开目标声明及其源码行号。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedPublicTarget {
    name: String,
    line: usize,
}

impl ParsedPublicTarget {
    /// 返回公开声明中的目标名称。
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回公开声明所在的一基行号。
    pub fn line(&self) -> usize {
        self.line
    }
}

/// 表示单个 Markfile 的未解析语法模型。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedModule {
    namespace: ModulePath,
    includes: Vec<ParsedInclude>,
    targets: Vec<ParsedTarget>,
    public_targets: Vec<ParsedPublicTarget>,
}

impl ParsedModule {
    /// 返回调用方赋予文件的模块路径。
    pub fn namespace(&self) -> &ModulePath {
        &self.namespace
    }

    /// 返回文件头部的导入声明。
    pub fn includes(&self) -> &[ParsedInclude] {
        &self.includes
    }

    /// 返回文件中声明的目标。
    pub fn targets(&self) -> &[ParsedTarget] {
        &self.targets
    }

    /// 返回文件尾部的公开声明。
    pub fn public_targets(&self) -> &[ParsedPublicTarget] {
        &self.public_targets
    }
}

/// 控制解析器是否向调用方交付语法模型。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseMode {
    Build,
    DryRun,
}

/// 表示诊断是否阻止构建。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticSeverity {
    Warning,
    Error,
}

/// 描述单文件解析产生的结构化诊断。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    severity: DiagnosticSeverity,
    line: usize,
    code: &'static str,
    message: String,
}

impl Diagnostic {
    // 创建不阻塞构建的警告。
    fn warning(line: usize, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            line,
            code,
            message: message.into(),
        }
    }

    // 创建阻塞构建的错误。
    fn error(line: usize, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Error,
            line,
            code,
            message: message.into(),
        }
    }

    /// 返回诊断严重级别。
    pub fn severity(&self) -> DiagnosticSeverity {
        self.severity
    }

    /// 返回诊断对应的一基行号。
    pub fn line(&self) -> usize {
        self.line
    }

    /// 返回稳定的诊断代码。
    pub fn code(&self) -> &'static str {
        self.code
    }

    /// 返回面向用户的诊断消息。
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// 汇总单文件解析模型与全部诊断。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseResult {
    module: Option<ParsedModule>,
    diagnostics: Vec<Diagnostic>,
    // 区分内部保留的部分模型与可交付模型。
    can_build: bool,
}

impl ParseResult {
    /// 在普通模式且无错误时借用语法模型。
    pub fn module(&self) -> Option<&ParsedModule> {
        self.can_build.then_some(self.module.as_ref()).flatten()
    }

    /// 在普通模式且无错误时取出语法模型。
    pub fn into_module(mut self) -> Option<ParsedModule> {
        self.can_build.then(|| self.module.take()).flatten()
    }

    /// 返回解析期间收集的全部诊断。
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// 判断是否存在阻塞构建的诊断。
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    }

    /// 向项目分析器提供用于继续扫描的部分模型。
    pub(crate) fn partial_module(&self) -> &ParsedModule {
        self.module
            .as_ref()
            .expect("the parser always retains a partial module")
    }
}

/// 按指定模式解析单个 Markfile 文本。
#[derive(Clone, Debug)]
pub struct Parser {
    mode: ParseMode,
}

impl Parser {
    /// 创建指定交付模式的解析器。
    pub fn new(mode: ParseMode) -> Self {
        Self { mode }
    }

    /// 扫描完整文本并返回模型与诊断。
    pub fn parse(&self, namespace: ModulePath, source: &str) -> ParseResult {
        let mut state = ParseState::new(namespace);
        for (index, line) in source.lines().enumerate() {
            state.consume_line(index + 1, line);
        }
        state.finish(self.mode)
    }
}

// 表示解析器当前所在的文件分区。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Section {
    Includes,
    Targets,
    PublicTargets,
}

// 表示当前目标仍在描述段或已进入规格段。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TargetBody {
    Description,
    Specifications,
}

// 暂存尚未遇到下一个目标边界的内容。
#[derive(Debug)]
struct PendingTarget {
    name: String,
    line: usize,
    dependencies: Vec<ParsedReference>,
    description: String,
    specifications: Vec<String>,
    // 防止进入规格段后重新接受描述文本。
    body: TargetBody,
}

impl PendingTarget {
    // 创建处于描述阶段的待完成目标。
    fn new(name: String, line: usize) -> Self {
        Self {
            name,
            line,
            dependencies: Vec::new(),
            description: String::new(),
            specifications: Vec::new(),
            body: TargetBody::Description,
        }
    }

    // 按源码顺序追加一行描述。
    fn append_description(&mut self, text: &str) {
        if !self.description.is_empty() {
            self.description.push('\n');
        }
        self.description.push_str(text);
    }

    // 将待完成目标转换为稳定语法节点。
    fn finish(self) -> ParsedTarget {
        ParsedTarget {
            name: self.name,
            line: self.line,
            dependencies: self.dependencies,
            description: self.description,
            specifications: self.specifications,
        }
    }
}

// 保存一次完整单文件扫描的可变状态。
struct ParseState {
    namespace: ModulePath,
    // 决定当前行应按哪类语法解释。
    section: Section,
    includes: Vec<ParsedInclude>,
    targets: Vec<ParsedTarget>,
    public_targets: Vec<ParsedPublicTarget>,
    target_names: BTreeSet<String>,
    // 保存跨行累积且尚未提交的目标。
    current_target: Option<PendingTarget>,
    diagnostics: Vec<Diagnostic>,
}

impl ParseState {
    // 初始化在导入分区中的解析状态。
    fn new(namespace: ModulePath) -> Self {
        Self {
            namespace,
            section: Section::Includes,
            includes: Vec::new(),
            targets: Vec::new(),
            public_targets: Vec::new(),
            target_names: BTreeSet::new(),
            current_target: None,
            diagnostics: Vec::new(),
        }
    }

    // 按当前分区解释一行非终止输入。
    fn consume_line(&mut self, line_number: usize, raw_line: &str) {
        let line = raw_line.trim_end();
        if line.trim().is_empty() {
            return;
        }
        if line == "---" {
            self.consume_separator(line_number);
            return;
        }
        match self.section {
            Section::Includes => self.consume_include(line_number, line),
            Section::Targets => self.consume_target_line(line_number, line),
            Section::PublicTargets => self.consume_public_target(line_number, line),
        }
    }

    // 消费分隔线并推进文件分区。
    fn consume_separator(&mut self, line_number: usize) {
        match self.section {
            Section::Includes => self.section = Section::Targets,
            Section::Targets => {
                self.finish_current_target();
                self.section = Section::PublicTargets;
            }
            Section::PublicTargets => self.diagnostics.push(Diagnostic::error(
                line_number,
                "E001",
                "a markfile has at most two section separators",
            )),
        }
    }

    // 解析导入分区中的目标路径和别名。
    fn consume_include(&mut self, line_number: usize, line: &str) {
        let Some(value) = directive_value(line) else {
            self.diagnostics.push(Diagnostic::error(
                line_number,
                "E002",
                "only `> path` directives are allowed before the first `---`",
            ));
            return;
        };
        let Some((path, alias)) = self.parse_include_value(line_number, value) else {
            return;
        };
        self.includes.push(ParsedInclude {
            target: ParsedReference::new(path, line_number),
            alias,
        });
    }

    // 解析目标声明、依赖、描述或规格。
    fn consume_target_line(&mut self, line_number: usize, line: &str) {
        if let Some(name) = target_name(line) {
            self.finish_current_target();
            if name.is_empty() {
                self.diagnostics.push(Diagnostic::error(
                    line_number,
                    "E003",
                    "a target declaration requires a name after `#`",
                ));
                return;
            }
            if !self.target_names.insert(name.to_owned()) {
                self.diagnostics.push(Diagnostic::error(
                    line_number,
                    "E004",
                    format!("target `{name}` is declared more than once"),
                ));
            }
            self.current_target = Some(PendingTarget::new(name.to_owned(), line_number));
            return;
        }

        if self.current_target.is_none() {
            self.diagnostics.push(Diagnostic::error(
                line_number,
                "E005",
                "a target declaration must appear before target content",
            ));
            return;
        }

        if let Some(value) = directive_value(line) {
            let path = self.parse_path(line_number, value);
            self.current_target
                .as_mut()
                .expect("a target was checked above")
                .dependencies
                .push(ParsedReference::new(path, line_number));
            return;
        }

        if let Some(specification) = specification_value(line) {
            let target = self
                .current_target
                .as_mut()
                .expect("a target was checked above");
            target.body = TargetBody::Specifications;
            target.specifications.push(specification.to_owned());
            return;
        }

        let target = self
            .current_target
            .as_mut()
            .expect("a target was checked above");
        if target.body == TargetBody::Specifications {
            self.diagnostics.push(Diagnostic::error(
                line_number,
                "E006",
                "target descriptions must precede specifications",
            ));
            return;
        }
        target.append_description(line);
    }

    // 解析公开分区中的本地目标名称。
    fn consume_public_target(&mut self, line_number: usize, line: &str) {
        let Some(name) = directive_value(line) else {
            self.diagnostics.push(Diagnostic::error(
                line_number,
                "E007",
                "only `> target-name` directives are allowed after the second `---`",
            ));
            return;
        };
        if name.is_empty() {
            self.diagnostics.push(Diagnostic::warning(
                line_number,
                "W001",
                "empty `>` directive",
            ));
            return;
        }
        if name.contains("::") {
            self.diagnostics.push(Diagnostic::error(
                line_number,
                "E008",
                "a public target must be declared by its local name",
            ));
            return;
        }
        self.public_targets.push(ParsedPublicTarget {
            name: name.to_owned(),
            line: line_number,
        });
    }

    // 将导入指令拆分为路径和可选别名。
    fn parse_include_value(
        &mut self,
        line_number: usize,
        value: &str,
    ) -> Option<(TargetPath, Option<String>)> {
        if value.is_empty() {
            return Some((self.parse_path(line_number, value), None));
        }
        let words = value.split_whitespace().collect::<Vec<_>>();
        match words.as_slice() {
            [path] => Some((self.parse_path(line_number, path), None)),
            [path, "as", alias] if !alias.contains("::") => Some((
                self.parse_path(line_number, path),
                Some((*alias).to_owned()),
            )),
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    line_number,
                    "E012",
                    "an include must use `> path` or `> path as alias`",
                ));
                None
            }
        }
    }

    // 拆分路径并为空指令记录警告。
    fn parse_path(&mut self, line_number: usize, path: &str) -> TargetPath {
        if path.is_empty() {
            self.diagnostics.push(Diagnostic::warning(
                line_number,
                "W001",
                "empty `>` directive",
            ));
            return TargetPath::new(Vec::new());
        }
        TargetPath::new(path.split("::").map(str::trim).map(str::to_owned).collect())
    }

    // 将当前目标提交到模块节点列表。
    fn finish_current_target(&mut self) {
        if let Some(target) = self.current_target.take() {
            self.targets.push(target.finish());
        }
    }

    // 完成结构校验并依据模式封装结果。
    fn finish(mut self, mode: ParseMode) -> ParseResult {
        self.finish_current_target();
        match self.section {
            Section::Includes => self.diagnostics.push(Diagnostic::error(
                1,
                "E009",
                "the include section must end with `---` before target declarations",
            )),
            Section::Targets => self.diagnostics.push(Diagnostic::error(
                1,
                "E010",
                "the target section must end with `---` before public target declarations",
            )),
            Section::PublicTargets => {}
        }
        for target in &self.public_targets {
            if !self.target_names.contains(target.name()) {
                self.diagnostics.push(Diagnostic::error(
                    target.line(),
                    "E011",
                    format!(
                        "public target `{}` is not declared in this module",
                        target.name()
                    ),
                ));
            }
        }
        let has_errors = self
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error);
        let can_build = !has_errors && mode == ParseMode::Build;
        let module = ParsedModule {
            namespace: self.namespace,
            includes: self.includes,
            targets: self.targets,
            public_targets: self.public_targets,
        };
        ParseResult {
            module: Some(module),
            diagnostics: self.diagnostics,
            can_build,
        }
    }
}

// 提取符合行首和空白规则的尖括号指令值。
fn directive_value(line: &str) -> Option<&str> {
    let value = line.strip_prefix('>')?;
    if value.is_empty() || value.starts_with(char::is_whitespace) {
        Some(value.trim())
    } else {
        None
    }
}

// 提取任意数量井号后的目标名称。
fn target_name(line: &str) -> Option<&str> {
    let name = line.strip_prefix('#')?;
    Some(name.trim_start_matches('#').trim())
}

// 提取符合空白规则的规格文本。
fn specification_value(line: &str) -> Option<&str> {
    let value = line.strip_prefix('-')?;
    if value.is_empty() || value.starts_with(char::is_whitespace) {
        Some(value.trim())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 为测试快速构造合法模块路径。
    fn path(path: &str) -> ModulePath {
        ModulePath::parse(path).unwrap()
    }

    // 验证普通模式构建模型并保留未解析路径。
    #[test]
    fn build_mode_parses_a_valid_module_and_preserves_unresolved_paths() {
        let source = ">
> crate::shared::lint as shared_lint
---
### build
> prepare
Build the documentation.\x20\x20
- The documentation renders successfully.
---
> build
";
        let result = Parser::new(ParseMode::Build).parse(path("guide"), source);
        let module = result.module().unwrap();

        assert!(!result.has_errors());
        assert_eq!(module.namespace(), &path("guide"));
        assert!(module.includes()[0].target().path().is_empty());
        assert_eq!(module.includes()[1].alias(), Some("shared_lint"));
        assert_eq!(
            module.includes()[1].target().path().segments(),
            &["crate", "shared", "lint"]
        );
        assert_eq!(module.targets()[0].line(), 4);
        assert_eq!(module.targets()[0].name(), "build");
        assert_eq!(module.targets()[0].dependencies()[0].line(), 5);
        assert_eq!(
            module.targets()[0].dependencies()[0].path().segments(),
            &["prepare"]
        );
        assert_eq!(
            module.targets()[0].description(),
            "Build the documentation."
        );
        assert_eq!(
            module.targets()[0].specifications(),
            &["The documentation renders successfully."]
        );
        assert_eq!(module.public_targets()[0].name(), "build");
        assert_eq!(
            result.diagnostics()[0].severity(),
            DiagnosticSeverity::Warning
        );
        assert_eq!(result.diagnostics()[0].line(), 1);
    }

    // 验证解析器收集全部错误且不交付错误模型。
    #[test]
    fn parser_continues_after_errors_and_withholds_the_module() {
        let source = "orphan
---
# build
- A specification.
Description after a specification.
---
# invalid-public-section
";
        let result = Parser::new(ParseMode::Build).parse(path("guide"), source);

        assert!(result.has_errors());
        assert!(result.module().is_none());
        assert_eq!(result.partial_module().targets().len(), 1);
        assert_eq!(result.diagnostics().len(), 3);
        assert_eq!(result.diagnostics()[0].code(), "E002");
        assert_eq!(result.diagnostics()[1].code(), "E006");
        assert_eq!(result.diagnostics()[2].code(), "E007");
    }

    // 验证试运行模式仅返回诊断。
    #[test]
    fn dry_run_never_returns_a_module_but_retains_diagnostics() {
        let result = Parser::new(ParseMode::DryRun).parse(
            path("guide"),
            "---
# build
Build the documentation.
---
> build
",
        );

        assert!(!result.has_errors());
        assert!(result.module().is_none());
        assert!(result.diagnostics().is_empty());
        assert!(result.into_module().is_none());
    }

    // 验证导入别名语法受到检查。
    #[test]
    fn include_alias_syntax_is_checked() {
        let result = Parser::new(ParseMode::DryRun).parse(
            path("guide"),
            "> crate::shared::lint alias lint
---
# build
---
> build
",
        );

        assert_eq!(result.diagnostics()[0].code(), "E012");
    }

    // 验证公开声明只接受非空本地名称。
    #[test]
    fn public_target_declarations_require_local_non_empty_names() {
        let result = Parser::new(ParseMode::DryRun).parse(
            path("guide"),
            "---
# build
---
> crate::guide::build
>
",
        );

        assert_eq!(result.diagnostics()[0].code(), "E008");
        assert_eq!(result.diagnostics()[1].code(), "W001");
    }

    // 验证文件必须包含两个分区边界。
    #[test]
    fn parser_requires_both_section_separators() {
        let result = Parser::new(ParseMode::DryRun).parse(path("guide"), "# build\n");

        assert_eq!(result.diagnostics()[0].code(), "E002");
        assert_eq!(result.diagnostics()[1].code(), "E009");
    }

    // 验证规格后描述报错且缩进标记视作文本。
    #[test]
    fn parser_rejects_descriptions_after_specifications_and_indented_markers_are_text() {
        let result = Parser::new(ParseMode::DryRun).parse(
            path("guide"),
            "---
# build
- First specification.
  # not-a-target
---
> build
",
        );

        assert_eq!(result.diagnostics()[0].code(), "E006");
    }

    // 验证重复目标和未声明公开目标报错。
    #[test]
    fn parser_rejects_duplicate_and_undefined_public_targets() {
        let result = Parser::new(ParseMode::DryRun).parse(
            path("guide"),
            "---
# build
# build
---
> missing
",
        );

        assert_eq!(result.diagnostics()[0].code(), "E004");
        assert_eq!(result.diagnostics()[1].code(), "E011");
    }
}
