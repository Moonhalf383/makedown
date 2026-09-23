use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::core::{Module, ModulePath, Project, Spec, Target, TargetId};
use crate::parser::{
    DiagnosticSeverity, ParseMode, ParsedInclude, ParsedModule, ParsedReference, Parser,
};

/// 控制项目分析是否交付核心内存对象。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnalysisMode {
    Build,
    DryRun,
}

/// 描述带文件位置的项目级结构化诊断。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnalysisDiagnostic {
    severity: DiagnosticSeverity,
    file: PathBuf,
    line: Option<usize>,
    code: &'static str,
    message: String,
}

impl AnalysisDiagnostic {
    // 将缺失模块错误映射到引用该模块的指令位置。
    pub(crate) fn at(&self, file: PathBuf, line: usize) -> Self {
        Self {
            file,
            line: Some(line),
            ..self.clone()
        }
    }

    /// 将单文件解析诊断附上文件路径。
    pub fn from_parse(file: PathBuf, diagnostic: &crate::parser::Diagnostic) -> Self {
        Self {
            severity: diagnostic.severity(),
            file,
            line: Some(diagnostic.line()),
            code: diagnostic.code(),
            message: diagnostic.message().to_owned(),
        }
    }

    /// 创建一条非阻塞的文件质量警告。
    pub fn warning(
        file: PathBuf,
        line: usize,
        code: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            file,
            line: Some(line),
            code,
            message: message.into(),
        }
    }

    // 创建一条阻塞项目构建的错误。
    fn error(
        file: PathBuf,
        line: Option<usize>,
        code: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: DiagnosticSeverity::Error,
            file,
            line,
            code,
            message: message.into(),
        }
    }

    /// 返回诊断严重级别。
    pub fn severity(&self) -> DiagnosticSeverity {
        self.severity
    }

    /// 返回诊断对应的 Markfile 路径。
    pub fn file(&self) -> &Path {
        &self.file
    }

    /// 返回可用的一基源码行号。
    pub fn line(&self) -> Option<usize> {
        self.line
    }

    /// 返回稳定的项目诊断代码。
    pub fn code(&self) -> &str {
        self.code
    }

    /// 返回面向用户的诊断消息。
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// 汇总项目核心模型与全部分析诊断。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnalysisResult {
    project: Option<Project>,
    diagnostics: Vec<AnalysisDiagnostic>,
}

impl AnalysisResult {
    /// 在普通模式且图健康时借用项目。
    pub fn project(&self) -> Option<&Project> {
        self.project.as_ref()
    }

    /// 在普通模式且图健康时取出项目。
    pub fn into_project(self) -> Option<Project> {
        self.project
    }

    /// 返回加载、解析和解析引用的全部诊断。
    pub fn diagnostics(&self) -> &[AnalysisDiagnostic] {
        &self.diagnostics
    }

    /// 判断是否存在阻塞项目构建的诊断。
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    }
}

/// 从根 Markfile 按需加载并解析健康依赖图。
#[derive(Clone, Debug)]
pub struct ProjectAnalyzer {
    root_file: PathBuf,
    mode: AnalysisMode,
}

impl ProjectAnalyzer {
    /// 以根 Markfile 的位置和分析模式创建分析器。
    pub fn new(root_file: impl Into<PathBuf>, mode: AnalysisMode) -> Self {
        Self {
            root_file: root_file.into(),
            mode,
        }
    }

    /// 从任意起始目标分析其模块可达图。
    pub fn analyze(&self, start: TargetId) -> AnalysisResult {
        self.analyze_with_progress(start, |_| Ok(()))
            .expect("the no-op progress observer cannot fail")
    }

    /// 在解析每个可读取的模块前通知调用方，并传播观察者错误。
    pub fn analyze_with_progress(
        &self,
        start: TargetId,
        mut progress: impl FnMut(&ModulePath) -> std::io::Result<()>,
    ) -> std::io::Result<AnalysisResult> {
        let mut state = AnalysisState::new(self, Some(start));
        state.load_reachable_modules(&mut progress, &mut |_, _| {}, &mut |path: &Path| {
            fs::read_to_string(path)
        })?;
        Ok(state.resolve())
    }

    /// 扫描项目目录内的所有模块并校验整个图。
    pub fn analyze_all_with_progress(
        &self,
        progress: impl FnMut(&ModulePath) -> std::io::Result<()>,
    ) -> std::io::Result<AnalysisResult> {
        self.analyze_all_with_modules(progress, |_, _| {})
    }

    // 在全量分析期间让检查器复用已解析的文件模型。
    pub(crate) fn analyze_all_with_modules(
        &self,
        mut progress: impl FnMut(&ModulePath) -> std::io::Result<()>,
        mut inspect: impl FnMut(&Path, &ParsedModule),
    ) -> std::io::Result<AnalysisResult> {
        let mut state = AnalysisState::new(self, None);
        let (modules, diagnostics) = self.discover_modules()?;
        state.pending = modules;
        state.diagnostics.extend(diagnostics);
        state.load_reachable_modules(&mut progress, &mut inspect, &mut |path: &Path| {
            fs::read_to_string(path)
        })?;
        Ok(state.resolve())
    }

    /// 从指定模块分析导入可达图，优先通过调用方提供的源码读取器获取文本。
    pub fn analyze_module_with_sources(
        &self,
        module: ModulePath,
        read_source: impl FnMut(&Path) -> std::io::Result<String>,
    ) -> std::io::Result<AnalysisResult> {
        self.analyze_module_with_sources_and_modules(module, read_source, |_, _| {})
    }

    // 向编辑器提供分析过程中已解析的模块以定位目标声明。
    pub(crate) fn analyze_module_with_sources_and_modules(
        &self,
        module: ModulePath,
        mut read_source: impl FnMut(&Path) -> std::io::Result<String>,
        mut inspect: impl FnMut(&Path, &ParsedModule),
    ) -> std::io::Result<AnalysisResult> {
        let mut state = AnalysisState::new(self, None);
        state.pending.push(module);
        state.load_reachable_modules(&mut |_| Ok(()), &mut inspect, &mut read_source)?;
        Ok(state.resolve())
    }

    // 根据项目根目录扫描全部 .mf 文件并按模块路径排序。
    fn discover_modules(&self) -> std::io::Result<(Vec<ModulePath>, Vec<AnalysisDiagnostic>)> {
        let root = self
            .root_file
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let canonical_root = root.canonicalize()?;
        let mut pending = vec![root.to_path_buf()];
        let mut visited = BTreeSet::new();
        let mut modules = vec![ModulePath::root()];
        let mut diagnostics = Vec::new();
        while let Some(directory) = pending.pop() {
            let canonical = match directory.canonicalize() {
                Ok(canonical) => canonical,
                Err(error) => {
                    diagnostics.push(AnalysisDiagnostic::error(
                        directory,
                        None,
                        "P018",
                        format!("cannot inspect directory: {error}"),
                    ));
                    continue;
                }
            };
            if !canonical.starts_with(&canonical_root) || !visited.insert(canonical) {
                continue;
            }
            let entries = match fs::read_dir(&directory) {
                Ok(entries) => entries,
                Err(error) => {
                    diagnostics.push(AnalysisDiagnostic::error(
                        directory,
                        None,
                        "P018",
                        format!("cannot read directory: {error}"),
                    ));
                    continue;
                }
            };
            let mut entries = entries.collect::<Vec<_>>();
            entries.sort_by_key(|entry| entry.as_ref().map(|entry| entry.path()).ok());
            for entry in entries {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        diagnostics.push(AnalysisDiagnostic::error(
                            directory.clone(),
                            None,
                            "P018",
                            format!("cannot read directory entry: {error}"),
                        ));
                        continue;
                    }
                };
                let path = entry.path();
                let metadata = match fs::metadata(&path) {
                    Ok(metadata) => metadata,
                    Err(error) => {
                        diagnostics.push(AnalysisDiagnostic::error(
                            path,
                            None,
                            "P018",
                            format!("cannot inspect path: {error}"),
                        ));
                        continue;
                    }
                };
                if metadata.is_dir() {
                    pending.push(path);
                } else if metadata.is_file()
                    && path.extension().is_some_and(|extension| extension == "mf")
                    && path != self.root_file
                {
                    let relative = path.strip_prefix(root).expect("file found inside root");
                    let mut segments = relative
                        .iter()
                        .map(|part| part.to_string_lossy().into_owned())
                        .collect::<Vec<_>>();
                    let last = segments.pop().expect("markfile has a filename");
                    segments.push(
                        last.strip_suffix(".mf")
                            .expect("markfile extension checked")
                            .to_owned(),
                    );
                    match ModulePath::new(segments) {
                        Ok(namespace) => {
                            if modules.contains(&namespace) {
                                diagnostics.push(AnalysisDiagnostic::error(
                                    path,
                                    None,
                                    "P017",
                                    format!("duplicate module path `{namespace}`"),
                                ));
                            } else {
                                modules.push(namespace);
                            }
                        }
                        Err(error) => diagnostics.push(AnalysisDiagnostic::error(
                            path,
                            None,
                            "P017",
                            format!("invalid module file path: {error}"),
                        )),
                    }
                }
            }
        }
        modules.sort();
        modules.reverse();
        Ok((modules, diagnostics))
    }

    // 将稳定模块路径映射为项目内的 Markfile 路径。
    fn module_file(&self, namespace: &ModulePath) -> PathBuf {
        if namespace.is_root() {
            return self.root_file.clone();
        }

        let mut path = self
            .root_file
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .to_path_buf();
        for segment in &namespace.segments()[..namespace.segments().len() - 1] {
            path.push(segment);
        }
        path.push(format!("{}.mf", namespace.segments().last().unwrap()));
        path
    }
}

// 保存一次按需项目分析的工作集和诊断。
struct AnalysisState<'a> {
    analyzer: &'a ProjectAnalyzer,
    start: Option<TargetId>,
    parsed_modules: BTreeMap<ModulePath, ParsedModule>,
    module_files: BTreeMap<ModulePath, PathBuf>,
    // 保存尚待加载的模块路径栈。
    pending: Vec<ModulePath>,
    // 防止导入环导致重复读取模块。
    visited: BTreeSet<ModulePath>,
    diagnostics: Vec<AnalysisDiagnostic>,
}

impl<'a> AnalysisState<'a> {
    // 以起始目标所在模块初始化待加载队列。
    fn new(analyzer: &'a ProjectAnalyzer, start: Option<TargetId>) -> Self {
        let pending = start
            .as_ref()
            .map(|id| vec![id.namespace().clone()])
            .unwrap_or_default();
        Self {
            analyzer,
            pending,
            start,
            parsed_modules: BTreeMap::new(),
            module_files: BTreeMap::new(),
            visited: BTreeSet::new(),
            diagnostics: Vec::new(),
        }
    }

    // 递归读取并解析导入可达的所有模块。
    fn load_reachable_modules(
        &mut self,
        progress: &mut impl FnMut(&ModulePath) -> std::io::Result<()>,
        inspect: &mut impl FnMut(&Path, &ParsedModule),
        read_source: &mut impl FnMut(&Path) -> std::io::Result<String>,
    ) -> std::io::Result<()> {
        while let Some(namespace) = self.pending.pop() {
            if !self.visited.insert(namespace.clone()) {
                continue;
            }

            let file = self.analyzer.module_file(&namespace);
            self.module_files.insert(namespace.clone(), file.clone());
            if !namespace.is_root() && same_file(&file, &self.analyzer.root_file) {
                self.diagnostics.push(AnalysisDiagnostic::error(
                    file,
                    None,
                    "P016",
                    format!("module `{namespace}` aliases the project root file"),
                ));
                continue;
            }
            let source = match read_source(&file) {
                Ok(source) => source,
                Err(error) => {
                    self.diagnostics.push(AnalysisDiagnostic::error(
                        file,
                        None,
                        "P001",
                        format!("cannot read module `{namespace}`: {error}"),
                    ));
                    continue;
                }
            };

            progress(&namespace)?;
            let parse_mode = match self.analyzer.mode {
                AnalysisMode::Build => ParseMode::Build,
                AnalysisMode::DryRun => ParseMode::DryRun,
            };
            let parse_result = Parser::new(parse_mode).parse(namespace.clone(), &source);
            self.diagnostics
                .extend(
                    parse_result
                        .diagnostics()
                        .iter()
                        .map(|diagnostic| AnalysisDiagnostic {
                            severity: diagnostic.severity(),
                            file: file.clone(),
                            line: Some(diagnostic.line()),
                            code: diagnostic.code(),
                            message: diagnostic.message().to_owned(),
                        }),
                );
            let module = parse_result.partial_module().clone();
            inspect(&file, &module);
            for include in module.includes() {
                if let Some(target) = self.resolve_path(module.namespace(), include.target(), &file)
                {
                    self.pending.push(target.namespace().clone());
                }
            }
            self.parsed_modules.insert(namespace, module);
        }
        Ok(())
    }

    // 将部分语法模型解析为核心项目并检查图健康。
    fn resolve(mut self) -> AnalysisResult {
        let mut project = Project::new(ModulePath::root());
        let namespaces = self.parsed_modules.keys().cloned().collect::<Vec<_>>();
        for namespace in namespaces {
            if let Some(module) = self.build_module(&namespace)
                && let Err(error) = project.add_module(module)
            {
                self.diagnostics.push(AnalysisDiagnostic::error(
                    self.file_for(&namespace),
                    None,
                    "P002",
                    error.to_string(),
                ));
            }
        }

        if self.start.is_some() {
            self.validate_start(&project);
        }
        self.detect_cycles(&project);
        let has_errors = self
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error);
        let project = (!has_errors && self.analyzer.mode == AnalysisMode::Build).then_some(project);
        AnalysisResult {
            project,
            diagnostics: self.diagnostics,
        }
    }

    // 解析单个模块的导入、依赖和可见性。
    fn build_module(&mut self, namespace: &ModulePath) -> Option<Module> {
        let parsed = self.parsed_modules.get(namespace)?.clone();
        let file = self.file_for(namespace);
        let local_names = parsed
            .targets()
            .iter()
            .map(|target| target.name().to_owned())
            .collect::<BTreeSet<_>>();
        let mut imports = BTreeMap::<String, TargetId>::new();

        for include in parsed.includes() {
            let Some(target) = self.resolve_path(namespace, include.target(), &file) else {
                continue;
            };
            if !self.validate_import(&target, include, &file) {
                continue;
            }
            let alias = include.alias().unwrap_or(target.name()).to_owned();
            if local_names.contains(&alias) {
                self.diagnostics.push(AnalysisDiagnostic::error(
                    file.clone(),
                    Some(include.target().line()),
                    "P003",
                    format!("import name `{alias}` is ambiguous; choose a unique `as` alias"),
                ));
            } else if let Some(existing) = imports.get(&alias) {
                if existing != &target {
                    self.diagnostics.push(AnalysisDiagnostic::error(
                        file.clone(),
                        Some(include.target().line()),
                        "P003",
                        format!("import name `{alias}` is ambiguous; choose a unique `as` alias"),
                    ));
                }
            } else {
                imports.insert(alias, target);
            }
        }

        let mut module = Module::new(namespace.clone());
        for imported_target in imports.values() {
            module.add_include(imported_target.clone());
        }
        for parsed_target in parsed.targets() {
            let mut target = Target::new(namespace.clone(), parsed_target.name());
            target.set_description(parsed_target.description());
            for specification in parsed_target.specifications() {
                target.add_specification(Spec::new(specification));
            }
            for dependency in parsed_target.dependencies() {
                if dependency.path().is_empty() {
                    continue;
                }
                let segments = dependency.path().segments();
                if segments.len() != 1 {
                    self.diagnostics.push(AnalysisDiagnostic::error(
                        file.clone(),
                        Some(dependency.line()),
                        "P004",
                        "target dependencies must use a local target or imported alias",
                    ));
                    continue;
                }
                let name = &segments[0];
                let resolved = if local_names.contains(name) {
                    Some(TargetId::new(namespace.clone(), name))
                } else {
                    imports.get(name).cloned()
                };
                match resolved {
                    Some(resolved) => target.add_dependency(resolved),
                    None => self.diagnostics.push(AnalysisDiagnostic::error(
                        file.clone(),
                        Some(dependency.line()),
                        "P005",
                        format!("dependency `{name}` is neither a local target nor an import"),
                    )),
                }
            }
            if let Err(error) = module.add_target(target) {
                self.diagnostics.push(AnalysisDiagnostic::error(
                    file.clone(),
                    Some(parsed_target.line()),
                    "P006",
                    error.to_string(),
                ));
            }
        }
        for public_target in parsed.public_targets() {
            if let Err(error) = module.publish_target(public_target.name()) {
                self.diagnostics.push(AnalysisDiagnostic::error(
                    file.clone(),
                    Some(public_target.line()),
                    "P007",
                    error.to_string(),
                ));
            }
        }
        Some(module)
    }

    // 校验导入目标存在且允许跨模块访问。
    fn validate_import(&mut self, target: &TargetId, include: &ParsedInclude, file: &Path) -> bool {
        let Some(module) = self.parsed_modules.get(target.namespace()) else {
            return false;
        };
        let Some(parsed_target) = module
            .targets()
            .iter()
            .find(|candidate| candidate.name() == target.name())
        else {
            self.diagnostics.push(AnalysisDiagnostic::error(
                file.to_owned(),
                Some(include.target().line()),
                "P008",
                format!("imported target `{target}` does not exist"),
            ));
            return false;
        };
        let is_public = module
            .public_targets()
            .iter()
            .any(|candidate| candidate.name() == parsed_target.name());
        if !is_public {
            self.diagnostics.push(AnalysisDiagnostic::error(
                file.to_owned(),
                Some(include.target().line()),
                "P009",
                format!("imported target `{target}` is private"),
            ));
            return false;
        }
        true
    }

    // 按文件目录语义将导入路径解析为绝对目标。
    fn resolve_path(
        &mut self,
        current: &ModulePath,
        reference: &ParsedReference,
        file: &Path,
    ) -> Option<TargetId> {
        let segments = reference.path().segments();
        if segments.is_empty() {
            return None;
        }
        if segments.len() < 2 {
            self.diagnostics.push(AnalysisDiagnostic::error(
                file.to_owned(),
                Some(reference.line()),
                "P010",
                "an include path requires an anchor and target name",
            ));
            return None;
        }

        let target_name = segments.last().unwrap().clone();
        let mut module_segments = match segments[0].as_str() {
            "crate" => Vec::new(),
            "self" => parent_segments(current),
            "super" => {
                let parent = parent_segments(current);
                if parent.is_empty() {
                    self.diagnostics.push(AnalysisDiagnostic::error(
                        file.to_owned(),
                        Some(reference.line()),
                        "P015",
                        "`super` escapes the project root",
                    ));
                    return None;
                }
                parent[..parent.len() - 1].to_vec()
            }
            _ => {
                self.diagnostics.push(AnalysisDiagnostic::error(
                    file.to_owned(),
                    Some(reference.line()),
                    "P011",
                    "an include path must start with `crate`, `self`, or `super`",
                ));
                return None;
            }
        };
        module_segments.extend_from_slice(&segments[1..segments.len() - 1]);
        let namespace = match ModulePath::new(module_segments) {
            Ok(namespace) => namespace,
            Err(error) => {
                self.diagnostics.push(AnalysisDiagnostic::error(
                    file.to_owned(),
                    Some(reference.line()),
                    "P012",
                    error.to_string(),
                ));
                return None;
            }
        };
        Some(TargetId::new(namespace, target_name))
    }

    // 校验用户指定的起始目标存在。
    fn validate_start(&mut self, project: &Project) {
        let start = self
            .start
            .as_ref()
            .expect("only called with a start target");
        let valid = project
            .module(start.namespace())
            .and_then(|module| module.target(start.name()))
            .is_some();
        if !valid {
            self.diagnostics.push(AnalysisDiagnostic::error(
                self.file_for(start.namespace()),
                None,
                "P013",
                format!("start target `{start}` does not exist"),
            ));
        }
    }

    // 对项目中的目标依赖执行完整环检测。
    fn detect_cycles(&mut self, project: &Project) {
        let mut states = BTreeMap::<TargetId, VisitState>::new();
        let mut stack = Vec::new();
        let target_ids = project
            .modules()
            .flat_map(|module| module.targets().map(|target| target.id().clone()))
            .collect::<Vec<_>>();
        for target in target_ids {
            self.visit_target(project, &target, &mut states, &mut stack);
        }
    }

    // 深度优先访问目标并报告回边形成的环。
    fn visit_target(
        &mut self,
        project: &Project,
        target_id: &TargetId,
        states: &mut BTreeMap<TargetId, VisitState>,
        stack: &mut Vec<TargetId>,
    ) {
        match states.get(target_id) {
            Some(VisitState::Complete) => return,
            Some(VisitState::Active) => {
                let start = stack.iter().position(|item| item == target_id).unwrap_or(0);
                let mut cycle = stack[start..].to_vec();
                cycle.push(target_id.clone());
                self.diagnostics.push(AnalysisDiagnostic::error(
                    self.file_for(target_id.namespace()),
                    None,
                    "P014",
                    format!(
                        "dependency cycle: {}",
                        cycle
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(" -> ")
                    ),
                ));
                return;
            }
            None => {}
        }
        states.insert(target_id.clone(), VisitState::Active);
        stack.push(target_id.clone());
        if let Some(target) = project
            .module(target_id.namespace())
            .and_then(|module| module.target(target_id.name()))
        {
            for dependency in target.dependencies() {
                self.visit_target(project, dependency, states, stack);
            }
        }
        stack.pop();
        states.insert(target_id.clone(), VisitState::Complete);
    }

    // 返回模块已记录或可推导的文件路径。
    fn file_for(&self, namespace: &ModulePath) -> PathBuf {
        self.module_files
            .get(namespace)
            .cloned()
            .unwrap_or_else(|| self.analyzer.module_file(namespace))
    }
}

// 标记深度优先遍历中的目标访问状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VisitState {
    Active,
    Complete,
}

// 返回模块文件所在目录的逻辑路径分段。
fn parent_segments(path: &ModulePath) -> Vec<String> {
    if path.is_root() {
        Vec::new()
    } else {
        path.segments()[..path.segments().len() - 1].to_vec()
    }
}

// 尽可能规范化路径后判断是否指向同一文件。
fn same_file(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    // 为测试快速构造合法模块路径。
    fn path(path: &str) -> ModulePath {
        ModulePath::parse(path).unwrap()
    }

    // 在临时目录中创建一组项目测试文件。
    fn fixture(files: &[(&str, &str)]) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("markfile-{unique}"));
        for (relative, content) in files {
            let file = root.join(relative);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, content).unwrap();
        }
        root
    }

    // 按名称读取示例元数据中的配置值。
    fn metadata_value<'a>(metadata: &'a str, name: &str) -> Option<&'a str> {
        metadata
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{name}=")))
    }

    // 将示例元数据中的绝对目标路径转换为目标标识。
    fn metadata_start(metadata: &str) -> TargetId {
        let mut segments = metadata_value(metadata, "start")
            .expect("example metadata must define a start target")
            .split("::")
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let name = segments.pop().expect("the start target cannot be empty");
        let namespace = if segments.is_empty() {
            ModulePath::root()
        } else {
            ModulePath::new(segments).unwrap()
        };
        TargetId::new(namespace, name)
    }

    // 按稳定顺序返回指定分类中的示例目录。
    fn example_directories(category: &str) -> Vec<PathBuf> {
        let mut directories = fs::read_dir(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("examples")
                .join(category),
        )
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
        directories.sort();
        directories
    }

    // 验证进度观察者收到每个模块且写入错误能够传播。
    #[test]
    fn progress_observer_receives_modules_and_propagates_errors() {
        let root = fixture(&[
            (
                "main.mf",
                "> crate::shared::check\n---\n# run\n> check\n---\n> run\n",
            ),
            ("shared.mf", "---\n# check\n---\n> check\n"),
        ]);
        let analyzer = ProjectAnalyzer::new(root.join("main.mf"), AnalysisMode::Build);
        let start = TargetId::new(ModulePath::root(), "run");
        let mut seen = Vec::new();

        analyzer
            .analyze_with_progress(start.clone(), |module| {
                seen.push(module.clone());
                Ok(())
            })
            .unwrap();
        let error = analyzer
            .analyze_with_progress(start, |_| Err(std::io::Error::other("log unavailable")))
            .unwrap_err();

        assert_eq!(seen, [ModulePath::root(), path("shared")]);
        assert_eq!(error.kind(), std::io::ErrorKind::Other);
        fs::remove_dir_all(root).unwrap();
    }

    // 验证仅加载起始模块的导入可达集合。
    #[test]
    fn loads_only_modules_reachable_from_the_start_module() {
        let root = fixture(&[
            ("main.mf", "---\n# root\n---\n> root\n"),
            (
                "app.mf",
                "> crate::shared::check\n---\n# run\n> check\n---\n> run\n",
            ),
            ("shared.mf", "---\n# check\n---\n> check\n"),
            ("broken.mf", "not valid"),
        ]);
        let start = TargetId::new(path("app"), "run");

        let result = ProjectAnalyzer::new(root.join("main.mf"), AnalysisMode::Build).analyze(start);
        let project = result.project().unwrap();

        assert!(!result.has_errors());
        assert!(project.module(&path("app")).is_some());
        assert!(project.module(&path("shared")).is_some());
        assert!(project.module(&path("broken")).is_none());
        fs::remove_dir_all(root).unwrap();
    }

    // 验证 self 与 super 采用文件目录语义。
    #[test]
    fn resolves_file_directory_self_and_super_paths() {
        let root = fixture(&[
            ("main.mf", "---\n# root_check\n---\n> root_check\n"),
            (
                "catalog/api.mf",
                "> self::model::check as model_check\n> super::root_check\n---\n# publish\n> model_check\n> root_check\n---\n> publish\n",
            ),
            ("catalog/model.mf", "---\n# check\n---\n> check\n"),
        ]);
        let start = TargetId::new(path("catalog::api"), "publish");

        let result = ProjectAnalyzer::new(root.join("main.mf"), AnalysisMode::Build).analyze(start);
        let target = result
            .project()
            .unwrap()
            .module(&path("catalog::api"))
            .unwrap()
            .target("publish")
            .unwrap();

        assert_eq!(
            target.dependencies(),
            &[
                TargetId::new(path("catalog::model"), "check"),
                TargetId::new(ModulePath::root(), "root_check"),
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }

    // 验证别名冲突、私有导入和依赖环诊断。
    #[test]
    fn reports_alias_collisions_private_imports_and_cycles() {
        let root = fixture(&[
            (
                "main.mf",
                "> crate::one::check\n> crate::two::check\n> crate::private::hidden\n---\n# run\n> check\n---\n> run\n",
            ),
            (
                "one.mf",
                "---\n# check\n> looped\n# looped\n> check\n---\n> check\n> looped\n",
            ),
            ("two.mf", "---\n# check\n---\n> check\n"),
            ("private.mf", "---\n# hidden\n---\n"),
        ]);
        let start = TargetId::new(ModulePath::root(), "run");

        let result = ProjectAnalyzer::new(root.join("main.mf"), AnalysisMode::Build).analyze(start);
        let codes = result
            .diagnostics()
            .iter()
            .map(AnalysisDiagnostic::code)
            .collect::<Vec<_>>();

        assert!(result.project().is_none());
        assert!(codes.contains(&"P003"));
        assert!(codes.contains(&"P009"));
        assert!(codes.contains(&"P014"));
        fs::remove_dir_all(root).unwrap();
    }

    // 验证项目试运行扫描同一可达图但不交付项目。
    #[test]
    fn dry_run_scans_the_same_reachable_graph_without_returning_a_project() {
        let root = fixture(&[("main.mf", "---\n# run\n---\n> run\n")]);
        let start = TargetId::new(ModulePath::root(), "run");

        let result =
            ProjectAnalyzer::new(root.join("main.mf"), AnalysisMode::DryRun).analyze(start);

        assert!(!result.has_errors());
        assert!(result.project().is_none());
        assert!(result.into_project().is_none());
        fs::remove_dir_all(root).unwrap();
    }

    // 验证试运行完整收集各类引用错误。
    #[test]
    fn resolver_reports_all_reference_failures_in_dry_run() {
        let root = fixture(&[
            (
                "main.mf",
                "> crate\n> unknown::thing\n> crate::..::target\n> crate::shared::missing\n---\n# run\n> crate::shared::ok\n> unknown\n---\n> run\n",
            ),
            ("shared.mf", "---\n# ok\n---\n> ok\n"),
        ]);
        let start = TargetId::new(ModulePath::root(), "run");

        let result =
            ProjectAnalyzer::new(root.join("main.mf"), AnalysisMode::DryRun).analyze(start);
        let codes = result
            .diagnostics()
            .iter()
            .map(AnalysisDiagnostic::code)
            .collect::<BTreeSet<_>>();

        assert!(result.project().is_none());
        for code in ["P004", "P005", "P008", "P010", "P011", "P012"] {
            assert!(codes.contains(code), "missing diagnostic {code}");
        }
        fs::remove_dir_all(root).unwrap();
    }

    // 验证相同目标的重复导入保持幂等。
    #[test]
    fn duplicate_imports_of_the_same_target_are_idempotent() {
        let root = fixture(&[
            (
                "main.mf",
                "> crate::shared::check\n> crate::shared::check\n---\n# run\n> check\n---\n> run\n",
            ),
            ("shared.mf", "---\n# check\n---\n> check\n"),
        ]);
        let start = TargetId::new(ModulePath::root(), "run");

        let result = ProjectAnalyzer::new(root.join("main.mf"), AnalysisMode::Build).analyze(start);

        assert!(!result.has_errors());
        assert_eq!(
            result.project().unwrap().root().unwrap().includes().len(),
            1
        );
        fs::remove_dir_all(root).unwrap();
    }

    // 验证根文件不能被另一模块路径重复加载。
    #[test]
    fn root_file_cannot_be_loaded_again_as_a_named_module() {
        let root = fixture(&[("main.mf", "> crate::main::root\n---\n# root\n---\n> root\n")]);
        let start = TargetId::new(ModulePath::root(), "root");

        let result = ProjectAnalyzer::new(root.join("main.mf"), AnalysisMode::Build).analyze(start);

        assert!(
            result
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code() == "P016")
        );
        fs::remove_dir_all(root).unwrap();
    }

    // 验证 super 无法越过项目根目录。
    #[test]
    fn super_cannot_escape_the_project_root() {
        let root = fixture(&[(
            "main.mf",
            "> super::outside::check\n---\n# run\n---\n> run\n",
        )]);
        let start = TargetId::new(ModulePath::root(), "run");

        let result = ProjectAnalyzer::new(root.join("main.mf"), AnalysisMode::Build).analyze(start);

        assert!(result.project().is_none());
        assert!(
            result
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code() == "P015")
        );
        fs::remove_dir_all(root).unwrap();
    }

    // 验证全部合法示例可解析为健康项目。
    #[test]
    fn all_valid_examples_resolve_to_healthy_projects() {
        for directory in example_directories("valid") {
            let metadata = fs::read_to_string(directory.join("expected.txt")).unwrap();
            let analyzer = ProjectAnalyzer::new(directory.join("main.mf"), AnalysisMode::Build);
            let result = analyzer.analyze(metadata_start(&metadata));
            assert!(
                !result.has_errors(),
                "{}: {:?}",
                directory.display(),
                result.diagnostics()
            );
            assert!(result.project().is_some());
        }
    }

    // 验证全部问题示例产生声明的诊断代码。
    #[test]
    fn all_invalid_examples_emit_expected_diagnostics() {
        for directory in example_directories("invalid") {
            let metadata = fs::read_to_string(directory.join("expected.txt")).unwrap();
            let analyzer = ProjectAnalyzer::new(directory.join("main.mf"), AnalysisMode::DryRun);
            let result = analyzer.analyze(metadata_start(&metadata));
            let actual = result
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.code().to_owned())
                .collect::<BTreeSet<_>>();
            let expected = metadata_value(&metadata, "codes")
                .expect("invalid example metadata must define diagnostic codes")
                .split(',')
                .map(str::to_owned)
                .collect::<BTreeSet<_>>();

            assert!(result.has_errors(), "{}", directory.display());
            assert!(
                expected.is_subset(&actual),
                "{}: expected {expected:?}, got {actual:?}",
                directory.display()
            );
        }
    }

    // 验证缺失模块和起始目标均被报告。
    #[test]
    fn missing_modules_and_start_targets_are_reported() {
        let root = fixture(&[(
            "main.mf",
            "> crate::missing::target\n---\n# run\n---\n> run\n",
        )]);
        let start = TargetId::new(ModulePath::root(), "absent");

        let result = ProjectAnalyzer::new(root.join("main.mf"), AnalysisMode::Build).analyze(start);
        let codes = result
            .diagnostics()
            .iter()
            .map(AnalysisDiagnostic::code)
            .collect::<Vec<_>>();

        assert!(codes.contains(&"P001"));
        assert!(codes.contains(&"P013"));
        assert_eq!(
            result.diagnostics()[0].severity(),
            DiagnosticSeverity::Error
        );
        assert!(result.diagnostics()[0].file().ends_with("missing.mf"));
        assert!(result.diagnostics()[0].line().is_none());
        assert!(!result.diagnostics()[0].message().is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
