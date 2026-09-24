use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

use crate::core::{Project, TargetId};

/// 保存一个目标在编译计划中所需的稳定数据。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedTarget {
    id: TargetId,
    dependencies: Vec<TargetId>,
    description: String,
    specifications: Vec<String>,
}

impl PlannedTarget {
    /// 返回目标的唯一标识。
    pub fn id(&self) -> &TargetId {
        &self.id
    }

    /// 返回目标的直接前置目标。
    pub fn dependencies(&self) -> &[TargetId] {
        &self.dependencies
    }

    /// 返回目标描述。
    pub fn description(&self) -> &str {
        &self.description
    }

    /// 返回目标的验收规格文本。
    pub fn specifications(&self) -> &[String] {
        &self.specifications
    }
}

/// 表示一组互不依赖且可以并行实施的目标。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompilationStage {
    index: usize,
    targets: Vec<PlannedTarget>,
}

impl CompilationStage {
    /// 返回从一开始的阶段编号。
    pub fn index(&self) -> usize {
        self.index
    }

    /// 返回阶段内按稳定顺序排列的目标。
    pub fn targets(&self) -> &[PlannedTarget] {
        &self.targets
    }
}

/// 表示从基础依赖到最终目标的分阶段实施计划。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompilationPlan {
    root_target: TargetId,
    stages: Vec<CompilationStage>,
}

impl CompilationPlan {
    /// 返回用户指定的最终目标。
    pub fn root_target(&self) -> &TargetId {
        &self.root_target
    }

    /// 返回从基础目标到最终目标排列的阶段。
    pub fn stages(&self) -> &[CompilationStage] {
        &self.stages
    }
}

/// 描述无法从核心项目生成编译计划的原因。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompileError {
    TargetNotFound(TargetId),
    DependencyCycle(Vec<TargetId>),
}

impl fmt::Display for CompileError {
    // 将编译错误渲染为可读文本。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TargetNotFound(target) => write!(formatter, "target `{target}` does not exist"),
            Self::DependencyCycle(targets) => write!(
                formatter,
                "dependency cycle prevents compilation; blocked targets: {}",
                targets
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }
}

impl Error for CompileError {}

/// 将健康项目中的可达依赖图转换为分阶段计划。
#[derive(Clone, Copy, Debug, Default)]
pub struct Compiler;

impl Compiler {
    /// 创建无额外配置的默认编译器。
    pub fn new() -> Self {
        Self
    }

    /// 使用默认渲染器将指定目标编译为 Markdown。
    pub fn compile(
        &self,
        project: &Project,
        root_target: TargetId,
    ) -> Result<String, CompileError> {
        let plan = self.plan(project, root_target)?;
        Ok(DefaultMarkdownRenderer::new().render(&plan))
    }

    /// 从指定目标生成叶节点优先的分阶段计划。
    pub fn plan(
        &self,
        project: &Project,
        root_target: TargetId,
    ) -> Result<CompilationPlan, CompileError> {
        let mut reachable = BTreeMap::new();
        collect_reachable(project, &root_target, &mut reachable)?;
        let mut remaining = reachable.keys().cloned().collect::<BTreeSet<_>>();
        let mut stages = Vec::new();

        while !remaining.is_empty() {
            let leaves = remaining
                .iter()
                .filter(|target| {
                    reachable
                        .get(*target)
                        .expect("remaining targets originate from the reachable map")
                        .dependencies
                        .iter()
                        .all(|dependency| !remaining.contains(dependency))
                })
                .cloned()
                .collect::<Vec<_>>();

            if leaves.is_empty() {
                return Err(CompileError::DependencyCycle(
                    remaining.into_iter().collect(),
                ));
            }

            let targets = leaves
                .iter()
                .map(|target| {
                    reachable
                        .get(target)
                        .expect("leaf targets originate from the reachable map")
                        .clone()
                })
                .collect();
            for leaf in leaves {
                remaining.remove(&leaf);
            }
            stages.push(CompilationStage {
                index: stages.len() + 1,
                targets,
            });
        }

        Ok(CompilationPlan {
            root_target,
            stages,
        })
    }
}

/// 将分阶段计划渲染为默认 Markdown 实施文档。
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultMarkdownRenderer;

impl DefaultMarkdownRenderer {
    /// 创建默认 Markdown 渲染器。
    pub fn new() -> Self {
        Self
    }

    /// 将编译计划渲染为人和 Agent 均可执行的文档。
    pub fn render(&self, plan: &CompilationPlan) -> String {
        let mut output = String::new();
        push_line(
            &mut output,
            &format!("# 规格实施计划：{}", plan.root_target()),
        );
        push_line(&mut output, "");
        push_line(&mut output, "本计划按照依赖关系分阶段排列。");
        push_line(&mut output, "");
        push_line(&mut output, "执行规则：");
        push_line(&mut output, "");
        push_line(&mut output, "1. 按阶段顺序实施。");
        push_line(
            &mut output,
            "2. 同一阶段中的目标互不依赖，可以并行实施，实际顺序由实施者决定。",
        );
        push_line(
            &mut output,
            "3. 开始下一阶段前，必须确认前序阶段的全部规格均已验收通过。",
        );
        push_line(&mut output, "4. 不得忽略未完成的前置目标。");

        for stage in plan.stages() {
            push_line(&mut output, "");
            push_line(
                &mut output,
                &format!("## 阶段 {}：{}", stage.index(), stage_title(stage, plan)),
            );
            for target in stage.targets() {
                render_target(&mut output, target);
            }
        }

        output
    }
}

// 递归收集起始目标可达的目标快照。
fn collect_reachable(
    project: &Project,
    target_id: &TargetId,
    reachable: &mut BTreeMap<TargetId, PlannedTarget>,
) -> Result<(), CompileError> {
    if reachable.contains_key(target_id) {
        return Ok(());
    }
    let target = project
        .module(target_id.namespace())
        .and_then(|module| module.target(target_id.name()))
        .ok_or_else(|| CompileError::TargetNotFound(target_id.clone()))?;
    let planned = PlannedTarget {
        id: target.id().clone(),
        dependencies: target.dependencies().to_vec(),
        description: target.description().to_owned(),
        specifications: target
            .specifications()
            .iter()
            .map(|specification| specification.content().to_owned())
            .collect(),
    };
    reachable.insert(target_id.clone(), planned.clone());
    for dependency in &planned.dependencies {
        collect_reachable(project, dependency, reachable)?;
    }
    Ok(())
}

// 根据阶段位置生成默认标题。
fn stage_title(stage: &CompilationStage, plan: &CompilationPlan) -> &'static str {
    if stage.index() == plan.stages().len() {
        "最终目标"
    } else if stage.index() == 1 {
        "基础目标"
    } else {
        "依赖目标"
    }
}

// 将单个目标渲染为 Markdown 任务章节。
fn render_target(output: &mut String, target: &PlannedTarget) {
    push_line(output, "");
    push_line(output, &format!("### 目标 `{}`", target.id()));

    if !target.dependencies().is_empty() {
        push_line(output, "");
        push_line(output, "#### 前置目标");
        push_line(output, "");
        push_line(
            output,
            "开始本目标前，必须确保以下目标的全部规格均已验收通过：",
        );
        push_line(output, "");
        for dependency in target.dependencies() {
            push_line(output, &format!("- `{dependency}`"));
        }
    }

    push_line(output, "");
    push_line(output, "#### 描述");
    push_line(output, "");
    if target.description().is_empty() {
        push_line(output, "（未提供描述）");
    } else {
        push_line(output, target.description());
    }

    push_line(output, "");
    push_line(output, "#### 验收规格");
    push_line(output, "");
    if target.specifications().is_empty() {
        push_line(output, "（未提供验收规格）");
    } else {
        for specification in target.specifications() {
            push_line(output, &format!("- [ ] {specification}"));
        }
    }
}

// 向输出追加一行并统一换行符。
fn push_line(output: &mut String, line: &str) {
    output.push_str(line);
    output.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Module, ModulePath, Spec, Target, TargetName};
    use crate::project::{AnalysisMode, ProjectAnalyzer};
    use std::fs;
    use std::path::{Path, PathBuf};

    // 为测试快速构造经过校验的目标标识。
    fn id(namespace: ModulePath, name: &str) -> TargetId {
        TargetId::new(namespace, TargetName::parse(name).unwrap())
    }

    // 为测试快速构造不附带内容的目标。
    fn make_target(namespace: ModulePath, name: &str) -> Target {
        Target::new(namespace, TargetName::parse(name).unwrap())
    }

    // 为测试快速构造合法模块路径。
    fn path(path: &str) -> ModulePath {
        ModulePath::parse(path).unwrap()
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
        id(namespace, &name)
    }

    // 按稳定顺序返回全部合法示例目录。
    fn valid_example_directories() -> Vec<PathBuf> {
        let mut directories = fs::read_dir(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("examples")
                .join("valid"),
        )
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
        directories.sort();
        directories
    }

    // 创建带描述、规格和依赖的测试目标。
    fn target(namespace: ModulePath, name: &str, dependencies: Vec<TargetId>) -> Target {
        let mut target = make_target(namespace, name);
        target.set_description(format!("{name} description"));
        target.add_specification(Spec::new(format!("{name} specification")));
        for dependency in dependencies {
            target.add_dependency(dependency);
        }
        target
    }

    // 创建包含多路径共享依赖的健康测试项目。
    fn healthy_project() -> Project {
        let root = ModulePath::root();
        let common = path("common");
        let mut root_module = Module::new(root.clone());
        let mut common_module = Module::new(common.clone());

        common_module
            .add_target(target(common.clone(), "base", Vec::new()))
            .unwrap();
        root_module
            .add_target(target(
                root.clone(),
                "left",
                vec![id(common.clone(), "base")],
            ))
            .unwrap();
        root_module
            .add_target(target(root.clone(), "right", vec![id(common, "base")]))
            .unwrap();
        root_module
            .add_target(target(
                root.clone(),
                "finish",
                vec![id(root.clone(), "left"), id(root, "right")],
            ))
            .unwrap();

        let mut project = Project::new(ModulePath::root());
        project.add_module(root_module).unwrap();
        project.add_module(common_module).unwrap();
        project
    }

    // 验证全部合法示例可以生成预期阶段数量的制品。
    #[test]
    fn all_valid_examples_compile_with_expected_stage_counts() {
        for directory in valid_example_directories() {
            let metadata = fs::read_to_string(directory.join("expected.txt")).unwrap();
            let start = metadata_start(&metadata);
            let analysis = ProjectAnalyzer::new(directory.join("main.mf"), AnalysisMode::Build)
                .analyze(start.clone());
            let project = analysis.into_project().unwrap();
            let plan = Compiler::new().plan(&project, start).unwrap();
            let expected = metadata_value(&metadata, "stages")
                .expect("valid example metadata must define stage count")
                .parse::<usize>()
                .unwrap();

            assert_eq!(plan.stages().len(), expected, "{}", directory.display());
            assert!(!DefaultMarkdownRenderer::new().render(&plan).is_empty());
        }
    }

    // 验证叶节点逐层剥离且共享依赖只出现一次。
    #[test]
    fn plan_groups_independent_targets_into_stable_stages() {
        let project = healthy_project();

        let plan = Compiler::new()
            .plan(&project, id(ModulePath::root(), "finish"))
            .unwrap();

        assert_eq!(plan.stages().len(), 3);
        assert_eq!(
            plan.stages()[0].targets()[0].id(),
            &id(path("common"), "base")
        );
        assert_eq!(
            plan.stages()[1]
                .targets()
                .iter()
                .map(|target| target.id().name())
                .collect::<Vec<_>>(),
            ["left", "right"]
        );
        assert_eq!(plan.stages()[2].targets()[0].id().name(), "finish");
        assert_eq!(
            plan.stages()
                .iter()
                .flat_map(|stage| stage.targets())
                .filter(|target| target.id().name() == "base")
                .count(),
            1
        );
        for stage in plan.stages() {
            let stage_ids = stage
                .targets()
                .iter()
                .map(PlannedTarget::id)
                .collect::<BTreeSet<_>>();
            assert!(stage.targets().iter().all(|target| {
                target
                    .dependencies()
                    .iter()
                    .all(|dependency| !stage_ids.contains(dependency))
            }));
        }
    }

    // 验证计划只包含起始目标可达的子图。
    #[test]
    fn plan_omits_unreachable_targets() {
        let mut project = healthy_project();
        project
            .root_mut()
            .unwrap()
            .add_target(target(ModulePath::root(), "unused", Vec::new()))
            .unwrap();

        let plan = Compiler::new()
            .plan(&project, id(ModulePath::root(), "left"))
            .unwrap();
        let names = plan
            .stages()
            .iter()
            .flat_map(|stage| stage.targets().iter())
            .map(|target| target.id().name())
            .collect::<Vec<_>>();

        assert_eq!(names, ["base", "left"]);
    }

    // 验证缺失目标会返回明确错误。
    #[test]
    fn plan_rejects_missing_targets() {
        let project = Project::new(ModulePath::root());
        let missing = id(ModulePath::root(), "missing");

        let error = Compiler::new().plan(&project, missing.clone()).unwrap_err();

        assert_eq!(error, CompileError::TargetNotFound(missing));
    }

    // 验证缺失的传递依赖也会返回明确错误。
    #[test]
    fn plan_rejects_missing_transitive_dependencies() {
        let root = ModulePath::root();
        let missing = id(root.clone(), "missing");
        let mut module = Module::new(root.clone());
        module
            .add_target(target(root.clone(), "build", vec![missing.clone()]))
            .unwrap();
        let mut project = Project::new(root.clone());
        project.add_module(module).unwrap();

        let error = Compiler::new()
            .plan(&project, id(root, "build"))
            .unwrap_err();

        assert_eq!(error, CompileError::TargetNotFound(missing));
    }

    // 验证编译器防御性地拒绝依赖环。
    #[test]
    fn plan_rejects_dependency_cycles() {
        let root = ModulePath::root();
        let mut module = Module::new(root.clone());
        module
            .add_target(target(root.clone(), "a", vec![id(root.clone(), "b")]))
            .unwrap();
        module
            .add_target(target(root.clone(), "b", vec![id(root.clone(), "a")]))
            .unwrap();
        let mut project = Project::new(root.clone());
        project.add_module(module).unwrap();

        let error = Compiler::new().plan(&project, id(root, "a")).unwrap_err();

        assert!(matches!(error, CompileError::DependencyCycle(_)));
    }

    // 验证目标不能直接依赖自身。
    #[test]
    fn plan_rejects_self_dependencies() {
        let root = ModulePath::root();
        let self_id = id(root.clone(), "recursive");
        let mut module = Module::new(root.clone());
        module
            .add_target(target(root.clone(), "recursive", vec![self_id.clone()]))
            .unwrap();
        let mut project = Project::new(root);
        project.add_module(module).unwrap();

        let error = Compiler::new().plan(&project, self_id).unwrap_err();

        assert!(matches!(error, CompileError::DependencyCycle(_)));
    }

    // 验证默认编译入口直接返回 Markdown 制品。
    #[test]
    fn compile_uses_the_default_markdown_renderer() {
        let project = healthy_project();

        let markdown = Compiler::new()
            .compile(&project, id(ModulePath::root(), "finish"))
            .unwrap();

        assert!(markdown.starts_with("# 规格实施计划：finish\n"));
        assert!(markdown.contains("## 阶段 3：最终目标"));
    }

    // 验证默认文档包含阶段、并行规则和任务细节。
    #[test]
    fn default_renderer_emits_an_actionable_markdown_plan() {
        let project = healthy_project();
        let plan = Compiler::new()
            .plan(&project, id(ModulePath::root(), "finish"))
            .unwrap();

        let markdown = DefaultMarkdownRenderer::new().render(&plan);

        assert!(markdown.starts_with("# 规格实施计划：finish\n"));
        assert!(markdown.contains("同一阶段中的目标互不依赖，可以并行实施"));
        assert!(markdown.contains("## 阶段 1：基础目标"));
        assert!(markdown.contains("## 阶段 3：最终目标"));
        assert!(markdown.contains("### 目标 `common::base`"));
        assert!(markdown.contains("- `left`\n- `right`"));
        assert!(markdown.contains("- [ ] finish specification"));
    }

    // 验证简单计划的默认 Markdown 布局保持稳定。
    #[test]
    fn default_renderer_has_a_stable_single_target_layout() {
        let mut module = Module::new(ModulePath::root());
        module
            .add_target(target(ModulePath::root(), "build", Vec::new()))
            .unwrap();
        let mut project = Project::new(ModulePath::root());
        project.add_module(module).unwrap();
        let plan = Compiler::new()
            .plan(&project, id(ModulePath::root(), "build"))
            .unwrap();

        let markdown = DefaultMarkdownRenderer::new().render(&plan);

        assert_eq!(
            markdown,
            "# 规格实施计划：build\n\
\n\
本计划按照依赖关系分阶段排列。\n\
\n\
执行规则：\n\
\n\
1. 按阶段顺序实施。\n\
2. 同一阶段中的目标互不依赖，可以并行实施，实际顺序由实施者决定。\n\
3. 开始下一阶段前，必须确认前序阶段的全部规格均已验收通过。\n\
4. 不得忽略未完成的前置目标。\n\
\n\
## 阶段 1：最终目标\n\
\n\
### 目标 `build`\n\
\n\
#### 描述\n\
\n\
build description\n\
\n\
#### 验收规格\n\
\n\
- [ ] build specification\n"
        );
    }

    // 验证空描述和空规格仍会生成明确章节。
    #[test]
    fn default_renderer_marks_missing_optional_content() {
        let mut module = Module::new(ModulePath::root());
        module
            .add_target(make_target(ModulePath::root(), "empty"))
            .unwrap();
        let mut project = Project::new(ModulePath::root());
        project.add_module(module).unwrap();
        let plan = Compiler::new()
            .plan(&project, id(ModulePath::root(), "empty"))
            .unwrap();

        let markdown = DefaultMarkdownRenderer::new().render(&plan);

        assert!(markdown.contains("（未提供描述）"));
        assert!(markdown.contains("（未提供验收规格）"));
    }
}
