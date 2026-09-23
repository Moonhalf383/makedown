use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

/// 表示一条保持原始自然语言的规格。
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Spec {
    content: String,
}

impl Spec {
    /// 从文本创建规格。
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
        }
    }

    /// 借用规格的文本内容。
    pub fn content(&self) -> &str {
        &self.content
    }

    /// 消耗规格并返回文本内容。
    pub fn into_content(self) -> String {
        self.content
    }
}

/// 表示项目根相对的规范模块路径。
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ModulePath {
    segments: Vec<String>,
}

impl ModulePath {
    /// 创建不含路径分段的根模块路径。
    pub fn root() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    /// 校验路径分段并创建模块路径。
    pub fn new(segments: Vec<String>) -> Result<Self, ModulePathError> {
        if let Some(segment) = segments
            .iter()
            .find(|segment| !is_valid_path_segment(segment))
        {
            return Err(ModulePathError::InvalidSegment(segment.clone()));
        }
        Ok(Self { segments })
    }

    /// 从双冒号分隔的逻辑路径创建模块路径。
    pub fn parse(path: &str) -> Result<Self, ModulePathError> {
        if path.is_empty() {
            return Err(ModulePathError::EmptyPath);
        }
        Self::new(path.split("::").map(str::to_owned).collect())
    }

    /// 返回模块路径的各级分段。
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// 判断该路径是否表示根模块。
    pub fn is_root(&self) -> bool {
        self.segments.is_empty()
    }
}

impl fmt::Display for ModulePath {
    // 将模块路径渲染为双冒号分隔形式。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.segments.join("::"))
    }
}

/// 描述模块路径构造失败的原因。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModulePathError {
    EmptyPath,
    InvalidSegment(String),
}

impl fmt::Display for ModulePathError {
    // 将模块路径错误渲染为可读文本。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPath => formatter.write_str("a module path cannot be empty"),
            Self::InvalidSegment(segment) => {
                write!(formatter, "`{segment}` is not a valid module path segment")
            }
        }
    }
}

impl Error for ModulePathError {}

/// 使用模块路径与名称唯一标识目标。
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetId {
    namespace: ModulePath,
    name: String,
}

impl TargetId {
    /// 组合模块路径和名称创建目标标识。
    pub fn new(namespace: ModulePath, name: impl Into<String>) -> Self {
        Self {
            namespace,
            name: name.into(),
        }
    }

    /// 返回目标所属的模块路径。
    pub fn namespace(&self) -> &ModulePath {
        &self.namespace
    }

    /// 返回目标的本地名称。
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for TargetId {
    // 将目标标识渲染为限定名称。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.namespace.is_root() {
            formatter.write_str(&self.name)
        } else {
            write!(formatter, "{}::{}", self.namespace, self.name)
        }
    }
}

// 判断分段能否安全映射为项目内路径。
fn is_valid_path_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && !segment
            .chars()
            .any(|character| character.is_whitespace() || matches!(character, '/' | '\\' | ':'))
}

/// 表示目标能否被其他模块引用。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Visibility {
    #[default]
    Private,
    Public,
}

/// 表示已解析并可参与依赖图的目标。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Target {
    id: TargetId,
    visibility: Visibility,
    dependencies: Vec<TargetId>,
    description: String,
    specifications: Vec<Spec>,
}

impl Target {
    /// 创建默认私有且内容为空的目标。
    pub fn new(namespace: ModulePath, name: impl Into<String>) -> Self {
        Self {
            id: TargetId::new(namespace, name),
            visibility: Visibility::Private,
            dependencies: Vec::new(),
            description: String::new(),
            specifications: Vec::new(),
        }
    }

    /// 返回目标的唯一标识。
    pub fn id(&self) -> &TargetId {
        &self.id
    }

    /// 返回目标所属的模块路径。
    pub fn namespace(&self) -> &ModulePath {
        self.id.namespace()
    }

    /// 返回目标的本地名称。
    pub fn name(&self) -> &str {
        self.id.name()
    }

    /// 返回目标当前的可见性。
    pub fn visibility(&self) -> Visibility {
        self.visibility
    }

    /// 判断目标是否可被其他模块引用。
    pub fn is_public(&self) -> bool {
        self.visibility == Visibility::Public
    }

    /// 设置目标的可见性。
    pub fn set_visibility(&mut self, visibility: Visibility) {
        self.visibility = visibility;
    }

    /// 返回目标的直接依赖。
    pub fn dependencies(&self) -> &[TargetId] {
        &self.dependencies
    }

    /// 添加一项尚未记录的直接依赖。
    pub fn add_dependency(&mut self, dependency: TargetId) {
        if !self.dependencies.contains(&dependency) {
            self.dependencies.push(dependency);
        }
    }

    /// 返回目标描述。
    pub fn description(&self) -> &str {
        &self.description
    }

    /// 替换目标描述。
    pub fn set_description(&mut self, description: impl Into<String>) {
        self.description = description.into();
    }

    /// 按新行向目标描述追加文本。
    pub fn append_description(&mut self, text: &str) {
        if !self.description.is_empty() {
            self.description.push('\n');
        }
        self.description.push_str(text);
    }

    /// 返回目标包含的规格。
    pub fn specifications(&self) -> &[Spec] {
        &self.specifications
    }

    /// 向目标末尾添加一条规格。
    pub fn add_specification(&mut self, specification: Spec) {
        self.specifications.push(specification);
    }
}

/// 聚合一个命名空间中的目标与导入。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Module {
    namespace: ModulePath,
    includes: Vec<TargetId>,
    targets: BTreeMap<String, Target>,
}

impl Module {
    /// 创建指定路径下的空模块。
    pub fn new(namespace: ModulePath) -> Self {
        Self {
            namespace,
            includes: Vec::new(),
            targets: BTreeMap::new(),
        }
    }

    /// 返回模块的逻辑路径。
    pub fn namespace(&self) -> &ModulePath {
        &self.namespace
    }

    /// 返回模块已解析的导入目标。
    pub fn includes(&self) -> &[TargetId] {
        &self.includes
    }

    /// 添加一项尚未记录的导入目标。
    pub fn add_include(&mut self, include: TargetId) {
        if !self.includes.contains(&include) {
            self.includes.push(include);
        }
    }

    /// 按稳定顺序遍历模块目标。
    pub fn targets(&self) -> impl Iterator<Item = &Target> {
        self.targets.values()
    }

    /// 按本地名称查找目标。
    pub fn target(&self, name: &str) -> Option<&Target> {
        self.targets.get(name)
    }

    /// 按本地名称可变地查找目标。
    pub fn target_mut(&mut self, name: &str) -> Option<&mut Target> {
        self.targets.get_mut(name)
    }

    /// 遍历模块公开的目标。
    pub fn public_targets(&self) -> impl Iterator<Item = &Target> {
        self.targets.values().filter(|target| target.is_public())
    }

    /// 将指定的本地目标设为公开。
    pub fn publish_target(&mut self, name: &str) -> Result<(), PublishTargetError> {
        let target = self
            .targets
            .get_mut(name)
            .ok_or_else(|| PublishTargetError::TargetNotFound(name.to_owned()))?;
        target.set_visibility(Visibility::Public);
        Ok(())
    }

    /// 校验命名空间后插入目标。
    pub fn add_target(&mut self, target: Target) -> Result<(), AddTargetError> {
        if target.namespace() != &self.namespace {
            return Err(AddTargetError::NamespaceMismatch {
                module: self.namespace.clone(),
                target: target.id().clone(),
            });
        }

        let name = target.name().to_owned();
        match self.targets.entry(name.clone()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(target);
                Ok(())
            }
            std::collections::btree_map::Entry::Occupied(_) => {
                Err(AddTargetError::DuplicateName(name))
            }
        }
    }
}

/// 描述向模块添加目标失败的原因。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AddTargetError {
    DuplicateName(String),
    NamespaceMismatch {
        module: ModulePath,
        target: TargetId,
    },
}

impl fmt::Display for AddTargetError {
    // 将添加目标错误渲染为可读文本。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateName(name) => write!(formatter, "target `{name}` already exists"),
            Self::NamespaceMismatch { module, target } => write!(
                formatter,
                "target `{target}` cannot be added to module `{module}` because their namespaces differ"
            ),
        }
    }
}

impl Error for AddTargetError {}

/// 描述公开目标失败的原因。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PublishTargetError {
    TargetNotFound(String),
}

impl fmt::Display for PublishTargetError {
    // 将公开目标错误渲染为可读文本。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TargetNotFound(name) => write!(formatter, "target `{name}` does not exist"),
        }
    }
}

impl Error for PublishTargetError {}

/// 聚合项目中已加载并解析的模块。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Project {
    root_module: ModulePath,
    modules: BTreeMap<ModulePath, Module>,
}

impl Project {
    /// 创建指定根模块的空项目。
    pub fn new(root_module: ModulePath) -> Self {
        Self {
            root_module,
            modules: BTreeMap::new(),
        }
    }

    /// 返回项目根模块路径。
    pub fn root_module(&self) -> &ModulePath {
        &self.root_module
    }

    /// 按模块路径查找模块。
    pub fn module(&self, namespace: &ModulePath) -> Option<&Module> {
        self.modules.get(namespace)
    }

    /// 按模块路径可变地查找模块。
    pub fn module_mut(&mut self, namespace: &ModulePath) -> Option<&mut Module> {
        self.modules.get_mut(namespace)
    }

    /// 按稳定顺序遍历项目模块。
    pub fn modules(&self) -> impl Iterator<Item = &Module> {
        self.modules.values()
    }

    /// 返回已加载的根模块。
    pub fn root(&self) -> Option<&Module> {
        self.module(&self.root_module)
    }

    /// 可变地返回已加载的根模块。
    pub fn root_mut(&mut self) -> Option<&mut Module> {
        self.modules.get_mut(&self.root_module)
    }

    /// 按路径插入一个尚未存在的模块。
    pub fn add_module(&mut self, module: Module) -> Result<(), AddModuleError> {
        let namespace = module.namespace().clone();
        match self.modules.entry(namespace.clone()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(module);
                Ok(())
            }
            std::collections::btree_map::Entry::Occupied(_) => {
                Err(AddModuleError::DuplicateNamespace(namespace))
            }
        }
    }
}

/// 描述向项目添加模块失败的原因。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AddModuleError {
    DuplicateNamespace(ModulePath),
}

impl fmt::Display for AddModuleError {
    // 将添加模块错误渲染为可读文本。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateNamespace(namespace) => {
                write!(formatter, "module `{namespace}` already exists")
            }
        }
    }
}

impl Error for AddModuleError {}

#[cfg(test)]
mod tests {
    use super::*;

    // 为测试快速构造合法模块路径。
    fn path(path: &str) -> ModulePath {
        ModulePath::parse(path).unwrap()
    }

    // 验证模块路径的分段、显示与根路径语义。
    #[test]
    fn module_path_is_a_normalized_relative_logical_path() {
        let module_path = path("catalog::api");

        assert_eq!(module_path.segments(), &["catalog", "api"]);
        assert_eq!(module_path.to_string(), "catalog::api");
        assert!(!module_path.is_root());
        assert!(ModulePath::root().is_root());
    }

    // 验证模块路径拒绝歧义和不安全分段。
    #[test]
    fn module_path_rejects_ambiguous_or_invalid_paths() {
        assert_eq!(ModulePath::parse(""), Err(ModulePathError::EmptyPath));
        assert_eq!(
            ModulePath::parse("catalog::::api"),
            Err(ModulePathError::InvalidSegment("".to_owned()))
        );
        assert_eq!(
            ModulePath::parse("catalog::api docs"),
            Err(ModulePathError::InvalidSegment("api docs".to_owned()))
        );
        assert_eq!(
            ModulePath::parse("catalog::../secret"),
            Err(ModulePathError::InvalidSegment("../secret".to_owned()))
        );
    }

    // 验证目标保留描述、依赖与规格。
    #[test]
    fn target_keeps_its_description_dependencies_and_specifications() {
        let mut target = Target::new(path("guide"), "publish");
        target.append_description("Build the guide.");
        target.append_description("Publish the result.");
        target.add_dependency(TargetId::new(path("guide"), "build"));
        target.add_dependency(TargetId::new(path("guide"), "build"));
        target.add_specification(Spec::new("The guide renders successfully."));

        assert_eq!(
            target.description(),
            "Build the guide.\nPublish the result."
        );
        assert_eq!(
            target.dependencies(),
            &[TargetId::new(path("guide"), "build")]
        );
        assert_eq!(
            target.specifications()[0].content(),
            "The guide renders successfully."
        );
    }

    // 验证重复目标不会覆盖已有目标。
    #[test]
    fn module_rejects_duplicate_target_names_without_replacing_the_original() {
        let mut module = Module::new(path("guide"));
        module
            .add_target(Target::new(path("guide"), "build"))
            .unwrap();

        let error = module
            .add_target(Target::new(path("guide"), "build"))
            .unwrap_err();

        assert_eq!(error, AddTargetError::DuplicateName("build".to_owned()));
        assert_eq!(
            module.target("build").unwrap().id(),
            &TargetId::new(path("guide"), "build")
        );
    }

    // 验证模块查询和导入去重行为。
    #[test]
    fn module_exposes_targets_and_deduplicates_includes() {
        let mut module = Module::new(path("guide"));
        let included_target = TargetId::new(path("common"), "lint");
        module.add_include(included_target.clone());
        module.add_include(included_target.clone());
        module
            .add_target(Target::new(path("guide"), "build"))
            .unwrap();
        module
            .target_mut("build")
            .unwrap()
            .set_description("Build the guide.");

        assert_eq!(module.namespace(), &path("guide"));
        assert_eq!(module.includes(), &[included_target]);
        assert_eq!(module.targets().count(), 1);
        assert_eq!(
            module.target("build").unwrap().description(),
            "Build the guide."
        );
        assert_eq!(Spec::new("content").into_content(), "content");
    }

    // 验证目标默认私有且可由模块公开。
    #[test]
    fn targets_are_private_until_the_module_publishes_them() {
        let mut module = Module::new(path("guide"));
        module
            .add_target(Target::new(path("guide"), "build"))
            .unwrap();
        module
            .add_target(Target::new(path("guide"), "draft"))
            .unwrap();

        module.publish_target("build").unwrap();

        assert!(module.target("build").unwrap().is_public());
        assert_eq!(
            module.target("draft").unwrap().visibility(),
            Visibility::Private
        );
        assert_eq!(
            module
                .public_targets()
                .map(Target::name)
                .collect::<Vec<_>>(),
            ["build"]
        );
    }

    // 验证公开不存在目标时返回错误。
    #[test]
    fn publishing_an_unknown_target_fails() {
        let mut module = Module::new(path("guide"));

        let error = module.publish_target("build").unwrap_err();

        assert_eq!(
            error,
            PublishTargetError::TargetNotFound("build".to_owned())
        );
    }

    // 验证模块拒绝其他命名空间的目标。
    #[test]
    fn module_rejects_a_target_from_another_namespace() {
        let mut module = Module::new(path("guide"));

        let error = module
            .add_target(Target::new(path("other"), "build"))
            .unwrap_err();

        assert_eq!(
            error,
            AddTargetError::NamespaceMismatch {
                module: path("guide"),
                target: TargetId::new(path("other"), "build"),
            }
        );
    }

    // 验证项目根模块不依赖具体文件名。
    #[test]
    fn project_models_an_entry_module_without_assuming_its_file_name() {
        let mut project = Project::new(path("application"));
        project
            .add_module(Module::new(path("application")))
            .unwrap();
        project.add_module(Module::new(path("library"))).unwrap();
        project
            .root_mut()
            .unwrap()
            .add_target(Target::new(path("application"), "run"))
            .unwrap();
        project
            .module_mut(&path("library"))
            .unwrap()
            .add_target(Target::new(path("library"), "serve"))
            .unwrap();

        assert_eq!(project.root_module(), &path("application"));
        assert_eq!(project.modules().count(), 2);
        assert_eq!(project.root().unwrap().target("run").unwrap().name(), "run");
        assert_eq!(
            project
                .module(&path("library"))
                .unwrap()
                .target("serve")
                .unwrap()
                .name(),
            "serve"
        );
    }

    // 验证项目拒绝重复模块路径。
    #[test]
    fn project_rejects_duplicate_module_namespaces() {
        let mut project = Project::new(path("application"));
        project
            .add_module(Module::new(path("application")))
            .unwrap();

        let error = project
            .add_module(Module::new(path("application")))
            .unwrap_err();

        assert_eq!(
            error,
            AddModuleError::DuplicateNamespace(path("application"))
        );
    }
}
