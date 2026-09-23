use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use lsp_types::{Location, Position, Range, Uri};
use url::Url;

use crate::core::{ModulePath, TargetId};
use crate::parser::{ParsedModule, ParsedReference};
use crate::project::{AnalysisDiagnostic, AnalysisMode, ProjectAnalyzer};

// 保存一次跨文件分析的诊断、语法模型与实际读取的源码。
pub(super) struct ProjectView {
    pub(super) diagnostics: Vec<AnalysisDiagnostic>,
    modules: BTreeMap<ModulePath, (PathBuf, ParsedModule)>,
    sources: HashMap<PathBuf, String>,
    current: ModulePath,
    root: PathBuf,
}

impl ProjectView {
    // 使用内存版本覆盖磁盘，并按打开文档所在模块加载导入可达图。
    pub(super) fn analyze(path: &Path, overlays: &HashMap<PathBuf, String>) -> Option<Self> {
        let root = path
            .ancestors()
            .find_map(|directory| {
                let candidate = directory.join("main.mf");
                (candidate.is_file() || overlays.contains_key(&candidate)).then_some(candidate)
            })
            .unwrap_or_else(|| path.to_path_buf());
        let current = if path == root {
            ModulePath::root()
        } else {
            let relative = path.strip_prefix(root.parent()?).ok()?;
            let mut segments = relative
                .iter()
                .map(|part| part.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            let name = segments.pop()?.strip_suffix(".mf")?.to_owned();
            segments.push(name);
            ModulePath::new(segments).ok()?
        };
        let sources = RefCell::new(HashMap::new());
        let modules = RefCell::new(BTreeMap::new());
        let analysis = ProjectAnalyzer::new(&root, AnalysisMode::DryRun)
            .analyze_module_with_sources_and_modules(
                current.clone(),
                |file| {
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
                },
                |file, module| {
                    modules.borrow_mut().insert(
                        module.namespace().clone(),
                        (file.to_path_buf(), module.clone()),
                    );
                },
            )
            .ok()?;
        Some(Self {
            diagnostics: analysis.diagnostics().to_vec(),
            modules: modules.into_inner(),
            sources: sources.into_inner(),
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
            TargetId::new(self.current.clone(), word)
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
                TargetId::new(self.current.clone(), &name[0])
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
            segments.last()?,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

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
}
