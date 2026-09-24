use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionTextEdit, Documentation, Location, MarkupContent,
    MarkupKind, Position, Range, TextEdit, Uri,
};
use url::Url;

use crate::core::{ModulePath, TargetId, TargetName};
use crate::parser::{ParsedModule, ParsedReference};
use crate::project::{AnalysisDiagnostic, AnalysisMode, ProjectAnalyzer};

// 保存一次跨文件分析的诊断、语法模型与实际读取的源码。
#[derive(Clone)]
pub(super) struct ProjectView {
    pub(super) diagnostics: Arc<Vec<AnalysisDiagnostic>>,
    modules: Arc<BTreeMap<ModulePath, (PathBuf, ParsedModule)>>,
    sources: Arc<HashMap<PathBuf, String>>,
    current: ModulePath,
    root: PathBuf,
}

impl ProjectView {
    // 从文件位置推导当前项目最近的根文件。
    pub(super) fn root_for(path: &Path, overlays: &HashMap<PathBuf, String>) -> PathBuf {
        path.ancestors()
            .find_map(|directory| {
                let candidate = directory.join("main.mf");
                (candidate.is_file() || overlays.contains_key(&candidate)).then_some(candidate)
            })
            .unwrap_or_else(|| path.to_path_buf())
    }

    // 使用内存版本覆盖磁盘，并按打开文档所在模块加载导入可达图。
    pub(super) fn analyze(path: &Path, overlays: &HashMap<PathBuf, String>) -> Option<Self> {
        Self::load(path, overlays, false)
    }

    // 为补全和引用查找索引整个项目及尚未落盘的模块。
    pub(super) fn index(path: &Path, overlays: &HashMap<PathBuf, String>) -> Option<Self> {
        Self::load(path, overlays, true)
    }

    // 返回用于区分嵌套项目的根文件位置。
    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    // 为同项目另一打开文件复用索引中的模块和源码。
    pub(super) fn for_path(&self, path: &Path) -> Option<Self> {
        let current = Self::module_for(path, &self.root)?;
        self.modules.get(&current)?;
        Some(Self {
            diagnostics: Arc::clone(&self.diagnostics),
            modules: Arc::clone(&self.modules),
            sources: Arc::clone(&self.sources),
            current,
            root: self.root.clone(),
        })
    }

    // 将项目根相对文件转换为稳定模块身份。
    fn module_for(path: &Path, root: &Path) -> Option<ModulePath> {
        if path == root {
            return Some(ModulePath::root());
        }
        let relative = path.strip_prefix(root.parent()?).ok()?;
        let mut segments = relative
            .iter()
            .map(|part| part.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let name = segments.pop()?.strip_suffix(".mf")?.to_owned();
        segments.push(name);
        ModulePath::new(segments).ok()
    }

    // 根据分析范围加载文件模型与项目诊断。
    fn load(path: &Path, overlays: &HashMap<PathBuf, String>, all: bool) -> Option<Self> {
        let root = Self::root_for(path, overlays);
        let current = Self::module_for(path, &root)?;
        let sources = RefCell::new(HashMap::new());
        let modules = RefCell::new(BTreeMap::new());
        let analyzer = ProjectAnalyzer::new(&root, AnalysisMode::DryRun);
        let read = |file: &Path| {
            let text = overlays
                .get(file)
                .or_else(|| {
                    file.canonicalize()
                        .ok()
                        .and_then(|path| overlays.get(&path))
                })
                .cloned()
                .map(Ok)
                .unwrap_or_else(|| fs::read_to_string(file))?;
            sources
                .borrow_mut()
                .insert(file.to_path_buf(), text.clone());
            Ok(text)
        };
        let inspect = |file: &Path, module: &ParsedModule| {
            modules.borrow_mut().insert(
                module.namespace().clone(),
                (file.to_path_buf(), module.clone()),
            );
        };
        let analysis = if all {
            let canonical_root = root.canonicalize().ok();
            let extra = overlays.keys().filter_map(|file| {
                if file == &root
                    || canonical_root
                        .as_ref()
                        .is_some_and(|root| file.canonicalize().ok().as_ref() == Some(root))
                {
                    return None;
                }
                let relative = file.strip_prefix(root.parent()?).ok()?;
                let mut segments = relative
                    .iter()
                    .map(|part| part.to_string_lossy().into_owned())
                    .collect::<Vec<_>>();
                let last = segments.pop()?.strip_suffix(".mf")?.to_owned();
                segments.push(last);
                ModulePath::new(segments).ok()
            });
            analyzer.analyze_all_with_sources_and_modules(read, inspect, extra)
        } else {
            analyzer.analyze_module_with_sources_and_modules(current.clone(), read, inspect)
        }
        .ok()?;
        Some(Self {
            diagnostics: Arc::new(analysis.diagnostics().to_vec()),
            modules: Arc::new(modules.into_inner()),
            sources: Arc::new(sources.into_inner()),
            current,
            root,
        })
    }

    // 返回当前文件的语义诊断并把直接缺失的导入定位到指令行。
    pub(super) fn diagnostics_for(&self, path: &Path) -> Vec<AnalysisDiagnostic> {
        let mut diagnostics = self
            .diagnostics
            .iter()
            .filter(|item| item.file() == path)
            .cloned()
            .collect::<Vec<_>>();
        if let Some((_, current)) = self.modules.get(&self.current) {
            for item in self.diagnostics.iter().filter(|item| item.code() == "P001") {
                for include in current.includes() {
                    if let Some(id) = self.resolve_path(include.target())
                        && self
                            .file_for(id.namespace())
                            .is_some_and(|file| file == item.file())
                    {
                        diagnostics.push(item.at(path.to_path_buf(), include.target().line()));
                    }
                }
            }
        }
        diagnostics
    }

    // 根据鼠标位置定位导入路径、别名或目标依赖的声明位置。
    pub(super) fn definition(&self, position: Position) -> Option<Location> {
        let (file, module) = self.modules.get(&self.current)?;
        let text = self.sources.get(file)?;
        let line = text
            .lines()
            .nth(usize::try_from(position.line).ok()?)?
            .trim_end_matches('\r');
        let line_number = usize::try_from(position.line).ok()? + 1;
        utf16_offset(line, position.character)?;
        let directive = line.strip_prefix('>')?.trim_start();
        let start = line.len() - directive.len();
        let word = directive.split_whitespace().next()?;
        let active = |start: usize, text: &str| {
            let first = u32::try_from(line[..start].encode_utf16().count()).ok()?;
            let last = first + u32::try_from(text.encode_utf16().count()).ok()?;
            (first..last).contains(&position.character).then_some(())
        };
        let target = if module
            .public_targets()
            .iter()
            .any(|item| item.line() == line_number)
        {
            active(start, word)?;
            TargetId::new(self.current.clone(), TargetName::parse(word).ok()?)
        } else if let Some(include) = module
            .includes()
            .iter()
            .find(|include| include.target().line() == line_number)
        {
            let alias = include.alias().and_then(|alias| {
                let offset = line.rfind(alias)?;
                active(offset, alias)
            });
            active(start, word).or(alias)?;
            self.resolve_path(include.target())?
        } else {
            let dependency = module
                .targets()
                .iter()
                .flat_map(|target| target.dependencies())
                .find(|item| item.line() == line_number)?;
            active(start, word)?;
            let name = dependency.path().segments();
            if name.len() != 1 {
                return None;
            }
            if module
                .targets()
                .iter()
                .any(|target| target.name() == name[0])
            {
                TargetId::new(self.current.clone(), TargetName::parse(&name[0]).ok()?)
            } else {
                let include = module.includes().iter().find(|item| {
                    item.alias()
                        .or_else(|| item.target().path().segments().last().map(String::as_str))
                        == Some(name[0].as_str())
                })?;
                self.resolve_path(include.target())?
            }
        };
        let (file, target_module) = self.modules.get(target.namespace())?;
        let declaration = target_module
            .targets()
            .iter()
            .find(|item| item.name() == target.name())?;
        if target.namespace() != &self.current
            && !target_module
                .public_targets()
                .iter()
                .any(|item| item.name() == target.name())
        {
            return None;
        }
        let text = self.sources.get(file)?;
        let declaration_line = text.lines().nth(declaration.line().checked_sub(1)?)?;
        let start = declaration_line.find(target.name())?;
        let first = u32::try_from(declaration_line[..start].encode_utf16().count()).ok()?;
        let end = first + u32::try_from(target.name().encode_utf16().count()).ok()?;
        let uri: Uri = Url::from_file_path(file).ok()?.as_str().parse().ok()?;
        let row = u32::try_from(declaration.line() - 1).ok()?;
        Some(Location {
            uri,
            range: Range::new(Position::new(row, first), Position::new(row, end)),
        })
    }

    // 返回光标所在目标声明或引用对应的唯一目标身份。
    pub(super) fn symbol_at(&self, position: Position) -> Option<TargetId> {
        let (file, module) = self.modules.get(&self.current)?;
        let text = self.sources.get(file)?;
        let row = usize::try_from(position.line).ok()?;
        let line = text.lines().nth(row)?.trim_end_matches('\r');
        utf16_offset(line, position.character)?;
        if line.starts_with('#') {
            let target = module
                .targets()
                .iter()
                .find(|target| target.line() == row + 1)?;
            let start = line.len() - line.trim_start_matches('#').trim_start().len();
            let first = u32::try_from(line[..start].encode_utf16().count()).ok()?;
            let last = first + u32::try_from(target.name().encode_utf16().count()).ok()?;
            return (first..last)
                .contains(&position.character)
                .then(|| TargetName::parse(target.name()).ok())
                .flatten()
                .map(|name| TargetId::new(self.current.clone(), name));
        }
        let location = self.definition(position)?;
        let file = Url::parse(location.uri.as_str())
            .ok()?
            .to_file_path()
            .ok()?;
        self.modules.iter().find_map(|(namespace, (path, module))| {
            (path == &file)
                .then(|| {
                    module
                        .targets()
                        .iter()
                        .find(|target| {
                            u32::try_from(target.line() - 1).ok() == Some(location.range.start.line)
                        })
                        .and_then(|target| {
                            TargetName::parse(target.name())
                                .ok()
                                .map(|name| TargetId::new(namespace.clone(), name))
                        })
                })
                .flatten()
        })
    }

    // 根据分区及光标前缀补全导入路径、目标依赖或公开声明。
    pub(super) fn completions(&self, position: Position) -> Vec<CompletionItem> {
        let Some((file, module)) = self.modules.get(&self.current) else {
            return Vec::new();
        };
        let Some(source) = self.sources.get(file) else {
            return Vec::new();
        };
        let Some(row) = usize::try_from(position.line).ok() else {
            return Vec::new();
        };
        let Some(line) = source.lines().nth(row) else {
            return Vec::new();
        };
        let line = line.trim_end_matches('\r');
        let Some(caret) = utf16_offset(line, position.character) else {
            return Vec::new();
        };
        let section = source
            .lines()
            .take(row)
            .filter(|line| line.trim_end() == "---")
            .count();
        if section > 2 || !line.starts_with('>') {
            return Vec::new();
        }
        let start = line.len() - line[1..].trim_start().len();
        if caret < start {
            return Vec::new();
        }
        let suffix = &line[start..];
        let token_end = suffix.find(char::is_whitespace).unwrap_or(suffix.len());
        if caret > start + token_end {
            return Vec::new();
        }
        let current = &line[start..caret];
        if current.contains(char::is_whitespace) {
            return Vec::new();
        }
        if section == 1
            && !module
                .targets()
                .iter()
                .any(|target| target.line() < row + 1)
        {
            return Vec::new();
        }
        let mut candidates = Vec::<Candidate>::new();
        match section {
            0 => {
                let parent = self
                    .current
                    .segments()
                    .split_last()
                    .map(|(_, parent)| parent)
                    .unwrap_or(&[]);
                let mut anchors = vec![("crate::", &[][..]), ("self::", parent)];
                if !parent.is_empty() {
                    anchors.push(("super::", &parent[..parent.len() - 1]));
                }
                for (anchor, base) in anchors {
                    candidates.push(Candidate::anchor(anchor.to_owned()));
                    for (namespace, (_, candidate)) in self.modules.iter() {
                        let Some(relative) = namespace.segments().strip_prefix(base) else {
                            continue;
                        };
                        let prefix = if relative.is_empty() {
                            anchor.to_owned()
                        } else {
                            format!("{anchor}{}::", relative.join("::"))
                        };
                        if !relative.is_empty() {
                            candidates.push(Candidate::module(
                                prefix.clone(),
                                Self::module_documentation(candidate),
                            ));
                        }
                        for target in candidate.targets().iter().filter(|target| {
                            candidate
                                .public_targets()
                                .iter()
                                .any(|item| item.name() == target.name())
                        }) {
                            candidates.push(Candidate::target(
                                format!("{prefix}{}", target.name()),
                                self.target_documentation(namespace, target.name()),
                            ));
                        }
                    }
                }
            }
            1 => {
                for target in module.targets() {
                    candidates.push(Candidate::target(
                        target.name().to_owned(),
                        self.target_documentation(&self.current, target.name()),
                    ));
                }
                for include in module.includes() {
                    if let Some(name) = include.alias().or_else(|| {
                        include
                            .target()
                            .path()
                            .segments()
                            .last()
                            .map(String::as_str)
                    }) {
                        let documentation = self
                            .resolve_path(include.target())
                            .and_then(|id| self.target_documentation(id.namespace(), id.name()));
                        candidates.push(Candidate::target(name.to_owned(), documentation));
                    }
                }
            }
            2 => {
                for target in module.targets() {
                    candidates.push(Candidate::target(
                        target.name().to_owned(),
                        self.target_documentation(&self.current, target.name()),
                    ));
                }
            }
            _ => return Vec::new(),
        }
        let first = u32::try_from(line[..start].encode_utf16().count()).unwrap_or(u32::MAX);
        let end =
            u32::try_from(line[..start + token_end].encode_utf16().count()).unwrap_or(u32::MAX);
        let range = Range::new(
            Position::new(position.line, first),
            Position::new(position.line, end),
        );
        candidates.sort_by(|a, b| (a.group, &a.label).cmp(&(b.group, &b.label)));
        candidates.dedup_by(|a, b| a.label == b.label);
        candidates
            .into_iter()
            .filter(|candidate| candidate.label.starts_with(current) && candidate.label != current)
            .map(|candidate| CompletionItem {
                text_edit: Some(CompletionTextEdit::Edit(TextEdit::new(
                    range,
                    candidate.label.clone(),
                ))),
                label: candidate.label.clone(),
                kind: Some(candidate.kind),
                sort_text: Some(format!("{}{}", candidate.group, candidate.label)),
                documentation: candidate.documentation.map(|value| {
                    Documentation::MarkupContent(MarkupContent {
                        kind: MarkupKind::PlainText,
                        value,
                    })
                }),
                ..Default::default()
            })
            .collect()
    }

    // 提取目标声明后到下一个目标或分区前的正文行作为预览文档。
    fn target_documentation(&self, namespace: &ModulePath, name: &str) -> Option<String> {
        let (file, module) = self.modules.get(namespace)?;
        let target = module
            .targets()
            .iter()
            .find(|target| target.name() == name)?;
        let text = self.sources.get(file)?;
        let mut body = Vec::new();
        for line in text.lines().skip(target.line()) {
            let line = line.trim_end_matches('\r');
            if line.starts_with('#') || line.trim_end() == "---" {
                break;
            }
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                body.push(trimmed.to_owned());
            }
        }
        (!body.is_empty()).then(|| body.join("\n"))
    }

    // 列出模块的公开目标作为模块路径候选的文档。
    fn module_documentation(module: &ParsedModule) -> Option<String> {
        let names = module
            .public_targets()
            .iter()
            .map(|target| format!("- {}", target.name()))
            .collect::<Vec<_>>();
        (!names.is_empty()).then(|| format!("public targets:\n{}", names.join("\n")))
    }

    // 在完整项目索引中收集同一目标的全部有效引用。
    pub(super) fn references(&self, target: &TargetId, include_declaration: bool) -> Vec<Location> {
        let mut locations = Vec::new();
        if include_declaration
            && let Some(view) = self.for_module(target.namespace())
            && let Some((file, module)) = view.modules.get(target.namespace())
            && let Some(declaration) = module
                .targets()
                .iter()
                .find(|item| item.name() == target.name())
        {
            let row = u32::try_from(declaration.line() - 1).unwrap_or(u32::MAX);
            if let Some(location) = view.location(file, row, target.name(), '#') {
                locations.push(location);
            }
        }
        for (namespace, (file, module)) in self.modules.iter() {
            let Some(view) = self.for_module(namespace) else {
                continue;
            };
            let public = self
                .modules
                .get(target.namespace())
                .is_some_and(|(_, module)| {
                    module
                        .public_targets()
                        .iter()
                        .any(|item| item.name() == target.name())
                });
            if namespace != target.namespace() && !public {
                continue;
            }
            for include in module.includes() {
                if view.resolve_path(include.target()).as_ref() == Some(target)
                    && let Some(location) = view.location(
                        file,
                        u32::try_from(include.target().line() - 1).unwrap_or(u32::MAX),
                        target.name(),
                        '>',
                    )
                {
                    locations.push(location);
                }
            }
            for local in module.targets() {
                for dependency in local.dependencies() {
                    let name = dependency.path().segments();
                    if name.len() != 1 {
                        continue;
                    }
                    let resolved = if module.targets().iter().any(|item| item.name() == name[0]) {
                        TargetName::parse(&name[0])
                            .ok()
                            .map(|name| TargetId::new(namespace.clone(), name))
                    } else {
                        module
                            .includes()
                            .iter()
                            .find(|item| {
                                item.alias().or_else(|| {
                                    item.target().path().segments().last().map(String::as_str)
                                }) == Some(name[0].as_str())
                            })
                            .and_then(|include| view.resolve_path(include.target()))
                    };
                    if resolved.as_ref() == Some(target)
                        && let Some(location) = view.location(
                            file,
                            u32::try_from(dependency.line() - 1).unwrap_or(u32::MAX),
                            &name[0],
                            '>',
                        )
                    {
                        locations.push(location);
                    }
                }
            }
            if namespace == target.namespace() {
                for public_target in module
                    .public_targets()
                    .iter()
                    .filter(|item| item.name() == target.name())
                {
                    if let Some(location) = view.location(
                        file,
                        u32::try_from(public_target.line() - 1).unwrap_or(u32::MAX),
                        target.name(),
                        '>',
                    ) {
                        locations.push(location);
                    }
                }
            }
        }
        locations.sort_by(|a, b| {
            (a.uri.as_str(), a.range.start.line, a.range.start.character).cmp(&(
                b.uri.as_str(),
                b.range.start.line,
                b.range.start.character,
            ))
        });
        locations.dedup();
        locations
    }

    // 在当前源文件指定行中构造目标名称的 UTF-16 位置。
    fn location(&self, file: &Path, row: u32, name: &str, marker: char) -> Option<Location> {
        let line = self
            .sources
            .get(file)?
            .lines()
            .nth(usize::try_from(row).ok()?)?;
        let start = if marker == '#' {
            let prefix = line.strip_prefix('#')?.trim_start_matches('#').trim_start();
            line.len() - prefix.len()
        } else {
            let rest = line.strip_prefix('>')?.trim_start();
            let start = line.len() - rest.len();
            if rest.starts_with(name) {
                start
            } else {
                let word = rest.split_whitespace().next()?;
                let suffix = word.strip_suffix(name)?;
                if !suffix.is_empty() && !suffix.ends_with("::") {
                    return None;
                }
                start + suffix.len()
            }
        };
        if !line[start..].starts_with(name) {
            return None;
        }
        let first = u32::try_from(line[..start].encode_utf16().count()).ok()?;
        let end = first + u32::try_from(name.encode_utf16().count()).ok()?;
        let uri: Uri = Url::from_file_path(file).ok()?.as_str().parse().ok()?;
        Some(Location {
            uri,
            range: Range::new(Position::new(row, first), Position::new(row, end)),
        })
    }

    // 以模块路径创建共享项目索引的局部视图。
    fn for_module(&self, module: &ModulePath) -> Option<Self> {
        let file = &self.modules.get(module)?.0;
        self.for_path(file)
    }

    // 将锚定导入路径解析为模块路径及目标名称。
    fn resolve_path(&self, reference: &ParsedReference) -> Option<TargetId> {
        let segments = reference.path().segments();
        if segments.len() < 2 {
            return None;
        }
        let parent = self
            .current
            .segments()
            .split_last()
            .map(|(_, parent)| parent.to_vec())
            .unwrap_or_default();
        let mut module = match segments[0].as_str() {
            "crate" => Vec::new(),
            "self" => parent,
            "super" => parent.get(..parent.len().checked_sub(1)?)?.to_vec(),
            _ => return None,
        };
        module.extend_from_slice(&segments[1..segments.len() - 1]);
        Some(TargetId::new(
            ModulePath::new(module).ok()?,
            TargetName::parse(segments.last()?).ok()?,
        ))
    }

    // 为直接缺失的导入推导预期的文件位置。
    fn file_for(&self, module: &ModulePath) -> Option<PathBuf> {
        if module.is_root() {
            return Some(self.root.clone());
        }
        let root = self.root.parent()?.to_path_buf();
        let mut file = root;
        for segment in &module.segments()[..module.segments().len() - 1] {
            file.push(segment);
        }
        file.push(format!("{}.mf", module.segments().last()?));
        Some(file)
    }
}

// 保存一个补全候选的标签、排序组、类型与可选文档。
struct Candidate {
    label: String,
    kind: CompletionItemKind,
    group: &'static str,
    documentation: Option<String>,
}

impl Candidate {
    // 创建 `crate::` 等路径锚点候选。
    fn anchor(label: String) -> Self {
        Self {
            label,
            kind: CompletionItemKind::KEYWORD,
            group: "0",
            documentation: None,
        }
    }

    // 创建目标候选并携带其正文预览。
    fn target(label: String, documentation: Option<String>) -> Self {
        Self {
            label,
            kind: CompletionItemKind::REFERENCE,
            group: "1",
            documentation,
        }
    }

    // 创建模块路径候选并携带公开目标列表。
    fn module(label: String, documentation: Option<String>) -> Self {
        Self {
            label,
            kind: CompletionItemKind::MODULE,
            group: "2",
            documentation,
        }
    }
}

// 将 LSP UTF-16 列号转换为不截断 Unicode 字符的源码字节位置。
fn utf16_offset(line: &str, column: u32) -> Option<usize> {
    let mut units = 0_u32;
    for (byte, character) in line.char_indices() {
        if units == column {
            return Some(byte);
        }
        units += u32::try_from(character.len_utf16()).ok()?;
        if units > column {
            return None;
        }
    }
    (units == column).then_some(line.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::TargetName;
    use std::time::{SystemTime, UNIX_EPOCH};

    // 为测试快速构造经过校验的目标标识。
    fn id(namespace: ModulePath, name: &str) -> TargetId {
        TargetId::new(namespace, TargetName::parse(name).unwrap())
    }

    // 创建隔离的项目目录以测试未保存内容和跨文件跳转。
    fn fixture() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "mkd-lsp-project-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    // 验证导入来源的未保存变更会更新可见性与目标存在性诊断。
    #[test]
    fn source_overlays_change_project_diagnostics() {
        let root = fixture();
        let main = root.join("main.mf");
        let shared = root.join("shared.mf");
        fs::write(
            &main,
            "> crate::shared::ready as gate\n---\n# build\n> gate\n- done\n---\n> build\n",
        )
        .unwrap();
        fs::write(&shared, "---\n# ready\n- done\n---\n> ready\n").unwrap();
        let mut overlays = HashMap::new();
        assert!(
            !ProjectView::analyze(&main, &overlays)
                .unwrap()
                .diagnostics_for(&main)
                .iter()
                .any(|item| item.code() == "P008" || item.code() == "P009")
        );
        overlays.insert(shared.clone(), "---\n# ready\n- done\n---\n".into());
        assert!(
            ProjectView::analyze(&main, &overlays)
                .unwrap()
                .diagnostics_for(&main)
                .iter()
                .any(|item| item.code() == "P009")
        );
        overlays.insert(
            shared.clone(),
            "---\n# later\n- done\n---\n> later\n".into(),
        );
        assert!(
            ProjectView::analyze(&main, &overlays)
                .unwrap()
                .diagnostics_for(&main)
                .iter()
                .any(|item| item.code() == "P008")
        );
        fs::remove_dir_all(root).unwrap();
    }

    // 验证尚未落盘的新模块通过打开文档覆盖层参与解析与跳转。
    #[test]
    fn new_unsaved_module_is_loaded_from_overlays() {
        let root = fixture();
        let main = root.join("main.mf");
        let new_module = root.join("fresh.mf");
        fs::write(
            &main,
            "> crate::fresh::ready\n---\n# build\n> ready\n---\n> build\n",
        )
        .unwrap();
        let overlays = HashMap::from([(
            new_module.clone(),
            "---\n# ready\n- done\n---\n> ready\n".into(),
        )]);
        let view = ProjectView::analyze(&main, &overlays).unwrap();
        assert!(
            view.diagnostics_for(&main)
                .iter()
                .all(|item| item.code() != "P001")
        );
        let location = view.definition(Position::new(3, 4)).unwrap();
        assert_eq!(
            Url::parse(location.uri.as_str())
                .unwrap()
                .to_file_path()
                .unwrap(),
            new_module
        );
        fs::remove_dir_all(root).unwrap();
    }

    // 验证依赖与导入别名可跳转到尚未保存的跨模块目标声明。
    #[test]
    fn definition_resolves_imports_dependencies_and_utf16() {
        let root = fixture();
        let main = root.join("main.mf");
        let shared = root.join("shared.mf");
        fs::write(
            &main,
            "> crate::shared::ready as gate\n---\n# build\n> gate\n- done\n---\n> build\n",
        )
        .unwrap();
        fs::write(&shared, "---\n# old\n- done\n---\n> old\n").unwrap();
        let overlays = HashMap::from([(
            shared.clone(),
            "---\n# ready\n- done\n---\n> ready\n".into(),
        )]);
        let view = ProjectView::analyze(&main, &overlays).unwrap();
        for position in [
            Position::new(0, 12),
            Position::new(0, 27),
            Position::new(3, 3),
        ] {
            let location = view.definition(position).unwrap();
            assert_eq!(
                Url::parse(location.uri.as_str())
                    .unwrap()
                    .to_file_path()
                    .unwrap(),
                shared
            );
            assert_eq!(location.range.start, Position::new(1, 2));
            assert_eq!(location.range.end, Position::new(1, 7));
        }
        let local = view.definition(Position::new(6, 4)).unwrap();
        assert_eq!(
            Url::parse(local.uri.as_str())
                .unwrap()
                .to_file_path()
                .unwrap(),
            main
        );
        assert_eq!(local.range.start, Position::new(2, 2));
        assert!(view.definition(Position::new(2, 2)).is_none());
        fs::remove_dir_all(root).unwrap();
    }

    // 验证同模块依赖及非 ASCII 目标声明位置采用 UTF-16 列号。
    #[test]
    fn definition_resolves_local_target_and_utf16_columns() {
        let root = fixture();
        let main = root.join("main.mf");
        fs::write(
            &main,
            "---\n# 测试😀\n- done\n# build\n> 测试😀\n- done\n---\n> build\n",
        )
        .unwrap();
        let view = ProjectView::analyze(&main, &HashMap::new()).unwrap();
        let location = view.definition(Position::new(4, 4)).unwrap();
        assert_eq!(location.range.start, Position::new(1, 2));
        assert_eq!(location.range.end, Position::new(1, 6));
        assert!(view.definition(Position::new(4, 0)).is_none());
        fs::remove_dir_all(root).unwrap();
    }

    // 验证找不到导入模块时诊断贴到打开文件的导入行。
    #[test]
    fn missing_module_diagnostic_is_visible_in_importer() {
        let root = fixture();
        let main = root.join("main.mf");
        fs::write(
            &main,
            "> crate::missing::ready\n---\n# build\n> ready\n---\n> build\n",
        )
        .unwrap();
        let view = ProjectView::analyze(&main, &HashMap::new()).unwrap();
        assert!(
            view.diagnostics_for(&main)
                .iter()
                .any(|item| item.code() == "P001" && item.line() == Some(1))
        );
        fs::remove_dir_all(root).unwrap();
    }

    // 验证补全根据分区提供路径、依赖或公开声明候选。
    #[test]
    fn completions_follow_section_context() {
        let root = fixture();
        let main = root.join("main.mf");
        let shared = root.join("shared.mf");
        fs::write(
            &main,
            "> crate::shared::ready as gate\n---\n# build\n> gate\n- done\n# hidden\n---\n> build\n",
        )
        .unwrap();
        fs::write(
            &shared,
            "---\n# ready\n- done\n# internal\n- hidden\n---\n> ready\n",
        )
        .unwrap();
        let view = ProjectView::index(&main, &HashMap::new()).unwrap();

        let imports = labels(&view.completions(Position::new(0, 2)));
        assert!(imports.contains(&"crate::shared::ready".to_owned()));
        assert!(imports.contains(&"crate::".to_owned()));
        assert!(imports.contains(&"self::".to_owned()));
        assert!(!imports.contains(&"super::".to_owned()));
        assert!(imports.contains(&"self::build".to_owned()));
        assert!(!imports.iter().any(|label| label.ends_with("::internal")));

        let narrowed = labels(&view.completions(Position::new(0, 10)));
        assert_eq!(
            narrowed,
            vec![
                "crate::shared::ready".to_owned(),
                "crate::shared::".to_owned(),
            ]
        );
        let narrowed_items = view.completions(Position::new(0, 10));
        assert_eq!(
            narrowed_items[0].sort_text.as_deref(),
            Some("1crate::shared::ready")
        );
        assert_eq!(
            narrowed_items[0].documentation,
            Some(lsp_types::Documentation::MarkupContent(
                lsp_types::MarkupContent {
                    kind: lsp_types::MarkupKind::PlainText,
                    value: "- done".to_owned(),
                }
            ))
        );
        assert_eq!(
            narrowed_items[1].documentation,
            Some(lsp_types::Documentation::MarkupContent(
                lsp_types::MarkupContent {
                    kind: lsp_types::MarkupKind::PlainText,
                    value: "public targets:\n- ready".to_owned(),
                }
            ))
        );

        let dependencies = view.completions(Position::new(3, 2));
        let dependency_labels = labels(&dependencies);
        assert!(dependency_labels.contains(&"gate".to_owned()));
        assert!(dependency_labels.contains(&"build".to_owned()));
        assert!(dependency_labels.contains(&"hidden".to_owned()));
        assert!(
            !dependency_labels
                .iter()
                .any(|label| label.contains("ready"))
        );
        let gate = dependencies
            .iter()
            .find(|item| item.label == "gate")
            .unwrap();
        assert_eq!(gate.sort_text.as_deref(), Some("1gate"));
        assert_eq!(
            gate.documentation,
            Some(lsp_types::Documentation::MarkupContent(
                lsp_types::MarkupContent {
                    kind: lsp_types::MarkupKind::PlainText,
                    value: "- done".to_owned(),
                }
            ))
        );

        let public = labels(&view.completions(Position::new(7, 2)));
        assert_eq!(public, vec!["build".to_owned(), "hidden".to_owned()]);

        assert!(view.completions(Position::new(1, 2)).is_empty());
        assert!(view.completions(Position::new(2, 2)).is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    // 验证嵌套目录的相对锚点以及未落盘模块的候选。
    #[test]
    fn completions_resolve_relative_anchors_and_overlay_modules() {
        let root = fixture();
        let main = root.join("main.mf");
        let nested = root.join("team").join("worker.mf");
        let sibling = root.join("team").join("shared.mf");
        fs::create_dir_all(nested.parent().unwrap()).unwrap();
        fs::write(&main, "---\n# root_target\n- done\n---\n> root_target\n").unwrap();
        fs::write(
            &nested,
            "> self::shared::ready\n---\n# work\n- done\n---\n> work\n",
        )
        .unwrap();
        let mut overlays = HashMap::new();
        overlays.insert(sibling, "---\n# ready\n- done\n---\n> ready\n".to_owned());
        let view = ProjectView::index(&nested, &overlays).unwrap();
        let self_candidates = labels(&view.completions(Position::new(0, 8)));
        assert!(self_candidates.contains(&"self::shared::ready".to_owned()));
        assert!(!self_candidates.contains(&"self::root_target".to_owned()));
        let super_candidates = labels(&view.completions(Position::new(0, 2)));
        assert!(super_candidates.contains(&"super::root_target".to_owned()));
        assert!(super_candidates.contains(&"crate::team::shared::ready".to_owned()));
        fs::remove_dir_all(root).unwrap();
    }

    // 验证 Unicode 目标名称在补全替换和引用范围中使用 UTF-16 列号。
    #[test]
    fn positions_use_utf16_for_unicode_target_names() {
        let root = fixture();
        let main = root.join("main.mf");
        fs::write(
            &main,
            "---\n# 😀build\n- done\n# use\n> 😀build\n> 😀b\n- done\n---\n> use\n",
        )
        .unwrap();
        let view = ProjectView::index(&main, &HashMap::new()).unwrap();
        let target = id(ModulePath::root(), "😀build");
        assert_eq!(view.symbol_at(Position::new(1, 2)), Some(target.clone()));
        assert_eq!(view.symbol_at(Position::new(1, 4)), Some(target.clone()));
        assert_eq!(view.symbol_at(Position::new(1, 3)), None);
        let locations = view.references(&target, true);
        assert_eq!(locations[0].range.start, Position::new(1, 2));
        assert_eq!(locations[0].range.end, Position::new(1, 9));
        assert_eq!(locations[1].range.start, Position::new(4, 2));
        assert_eq!(locations[1].range.end, Position::new(4, 9));
        let items = view.completions(Position::new(5, 4));
        assert_eq!(labels(&items), vec!["😀build".to_owned()]);
        let CompletionTextEdit::Edit(edit) = items[0].text_edit.as_ref().unwrap() else {
            panic!("expected full-token replacement");
        };
        assert_eq!(edit.range.start, Position::new(5, 2));
        assert_eq!(edit.range.end, Position::new(5, 5));
        assert!(view.completions(Position::new(5, 3)).is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    // 提取补全候选的标签列表。
    fn labels(items: &[CompletionItem]) -> Vec<String> {
        items.iter().map(|item| item.label.clone()).collect()
    }

    // 验证引用查找覆盖导入、依赖、公开声明并尊重可见性。
    #[test]
    fn references_cover_imports_dependencies_and_declarations() {
        let root = fixture();
        let main = root.join("main.mf");
        let shared = root.join("shared.mf");
        fs::write(
            &main,
            "> crate::shared::ready as gate\n---\n# build\n> gate\n- done\n---\n> build\n",
        )
        .unwrap();
        fs::write(&shared, "---\n# ready\n- done\n---\n> ready\n").unwrap();
        let view = ProjectView::index(&main, &HashMap::new()).unwrap();
        let ready = id(ModulePath::parse("shared").unwrap(), "ready");

        let target = view.symbol_at(Position::new(3, 3)).unwrap();
        assert_eq!(target, ready);
        assert_eq!(view.symbol_at(Position::new(1, 2)), None);

        let mut references = view.references(&ready, true);
        assert_eq!(references.len(), 4);
        let include = references.remove(0);
        let dependency = references.remove(0);
        let declaration = references.remove(0);
        let public = references.remove(0);
        assert_eq!(
            Url::parse(include.uri.as_str())
                .unwrap()
                .to_file_path()
                .unwrap(),
            main
        );
        assert_eq!(include.range.start, Position::new(0, 17));
        assert_eq!(
            Url::parse(dependency.uri.as_str())
                .unwrap()
                .to_file_path()
                .unwrap(),
            main
        );
        assert_eq!(dependency.range.start, Position::new(3, 2));
        assert_eq!(
            Url::parse(declaration.uri.as_str())
                .unwrap()
                .to_file_path()
                .unwrap(),
            root.join("shared.mf")
        );
        assert_eq!(declaration.range.start, Position::new(1, 2));
        assert_eq!(
            Url::parse(public.uri.as_str())
                .unwrap()
                .to_file_path()
                .unwrap(),
            root.join("shared.mf")
        );
        assert_eq!(public.range.start, Position::new(4, 2));

        let without_declaration = view.references(&ready, false);
        assert_eq!(without_declaration.len(), 3);
        fs::remove_dir_all(root).unwrap();
    }
}
