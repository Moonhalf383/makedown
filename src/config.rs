use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::core::{ModulePath, TargetId, TargetName};

/// 保存格式版本与命名构建配方。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    version: u32,
    #[serde(default)]
    build: BuildConfig,
}

/// 保存默认配方名及全部命名配方。
#[derive(Debug, Default, Deserialize)]
pub struct BuildConfig {
    #[serde(default)]
    default: Option<String>,
    #[serde(flatten)]
    profiles: BTreeMap<String, BuildProfile>,
}

/// 描述一个可以独立构建的入口及制品。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildProfile {
    target: String,
    output: PathBuf,
    template: Option<PathBuf>,
}

/// 汇总配置解析和路径校验错误。
#[derive(Debug)]
pub enum ConfigError {
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    Invalid {
        path: PathBuf,
        message: String,
    },
}

impl fmt::Display for ConfigError {
    // 渲染带配置文件路径的错误说明。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(formatter, "cannot read `{}`: {source}", path.display())
            }
            Self::Parse { path, source } => {
                write!(formatter, "invalid config `{}`: {source}", path.display())
            }
            Self::Invalid { path, message } => {
                write!(formatter, "invalid config `{}`: {message}", path.display())
            }
        }
    }
}

impl Error for ConfigError {}

impl ProjectConfig {
    /// 从入口 Markfile 所在目录读取配置；不存在时返回 None。
    pub fn load(root_file: &Path) -> Result<Option<Self>, ConfigError> {
        let path = root_file
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("mkd.toml");
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && fs::symlink_metadata(&path).is_err_and(|metadata_error| {
                        metadata_error.kind() == std::io::ErrorKind::NotFound
                    }) =>
            {
                return Ok(None);
            }
            Err(source) => return Err(ConfigError::Read { path, source }),
        };
        Self::parse(&path, &source).map(Some)
    }

    /// 校验配置格式、默认配方、目标与路径后创建配置对象。
    pub fn parse(path: &Path, source: &str) -> Result<Self, ConfigError> {
        let config: Self = toml::from_str(source).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
        let invalid = |message: String| ConfigError::Invalid {
            path: path.to_path_buf(),
            message,
        };
        if config.version != 1 {
            return Err(invalid(format!(
                "unsupported version {}; expected 1",
                config.version
            )));
        }
        if !config.build.profiles.is_empty() && config.build.default.is_none() {
            return Err(invalid(
                "[build] default is required when profiles exist".into(),
            ));
        }
        if let Some(default) = &config.build.default
            && !config.build.profiles.contains_key(default)
        {
            return Err(invalid(format!(
                "default profile `{default}` does not exist"
            )));
        }
        let root = path.parent().unwrap_or_else(|| Path::new("."));
        for (name, profile) in &config.build.profiles {
            if name.is_empty() || name.contains('.') || name.chars().any(char::is_whitespace) {
                return Err(invalid(format!("invalid profile name `{name}`")));
            }
            profile
                .target_id()
                .map_err(|message| invalid(format!("[build.{name}] {message}")))?;
            project_path(root, &profile.output)
                .map_err(|message| invalid(format!("[build.{name}] output: {message}")))?;
            if let Some(template) = &profile.template {
                project_path(root, template)
                    .map_err(|message| invalid(format!("[build.{name}] template: {message}")))?;
            }
        }
        Ok(config)
    }

    /// 按可选名称选取构建配方，省略名称时选默认配方。
    pub fn profile(&self, name: Option<&str>) -> Option<(&str, &BuildProfile)> {
        let name = name.or(self.build.default.as_deref())?;
        self.build
            .profiles
            .get_key_value(name)
            .map(|(key, value)| (key.as_str(), value))
    }
}

impl BuildProfile {
    /// 将配置的限定目标转换为已校验的身份。
    pub fn target_id(&self) -> Result<TargetId, String> {
        let mut segments = self
            .target
            .split("::")
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let name = segments.pop().unwrap_or_default();
        let name = TargetName::parse(&name).map_err(|error| error.to_string())?;
        let namespace = ModulePath::new(segments).map_err(|error| error.to_string())?;
        Ok(TargetId::new(namespace, name))
    }

    /// 返回配置指定的模板相对路径。
    pub fn template(&self) -> Option<&Path> {
        self.template.as_deref()
    }

    /// 返回配置指定的输出相对路径。
    pub fn output(&self) -> &Path {
        &self.output
    }
}

/// 校验配置路径确实落在项目内部，并返回以项目目录为基准的路径。
pub fn project_path(root: &Path, value: &Path) -> Result<PathBuf, String> {
    if value.as_os_str().is_empty() || value.is_absolute() {
        return Err("expected a nonempty project-relative path".into());
    }
    let mut depth = 0usize;
    for part in value.components() {
        match part {
            Component::Normal(_) => depth += 1,
            Component::ParentDir if depth > 0 => depth -= 1,
            Component::CurDir => {}
            _ => return Err("path escapes the project directory".into()),
        }
    }
    if depth == 0 {
        return Err("path must refer to a file inside the project".into());
    }
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("cannot inspect project directory: {error}"))?;
    let path = root.join(value);
    let mut existing = path.as_path();
    while let Err(error) = fs::symlink_metadata(existing) {
        if error.kind() != std::io::ErrorKind::NotFound {
            return Err(format!("cannot inspect path: {error}"));
        }
        existing = existing.parent().ok_or("cannot inspect path ancestors")?;
    }
    let canonical_existing = existing
        .canonicalize()
        .map_err(|error| format!("cannot inspect path: {error}"))?;
    if !canonical_existing.starts_with(&canonical_root) {
        return Err("path follows a symbolic link outside the project".into());
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 验证配置路径不能越过项目或通过符号链接间接逃逸。
    #[test]
    fn project_paths_remain_inside_the_root() {
        let root = std::env::temp_dir().join(format!("mkd-config-path-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        assert!(project_path(&root, Path::new("dist/plan.md")).is_ok());
        assert!(project_path(&root, Path::new("../outside.md")).is_err());
        assert!(project_path(&root, Path::new("/tmp/other.md")).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(std::env::temp_dir(), root.join("outside")).unwrap();
            assert!(project_path(&root, Path::new("outside/plan.md")).is_err());
            std::os::unix::fs::symlink(root.join("missing"), root.join("broken")).unwrap();
            assert!(project_path(&root, Path::new("broken")).is_err());
        }
        fs::remove_dir_all(root).unwrap();
    }

    // 验证默认配方及字段严格校验。
    #[test]
    fn validates_profiles_and_unknown_fields() {
        let root = std::env::temp_dir().join(format!("mkd-config-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("mkd.toml");
        let source = "version = 1\n[build]\ndefault = 'release'\n[build.release]\ntarget = 'publish'\noutput = 'dist/out.md'\n";
        let config = ProjectConfig::parse(&path, source).unwrap();
        assert_eq!(
            config.profile(None).unwrap().1.target_id().unwrap().name(),
            "publish"
        );
        assert!(ProjectConfig::parse(&path, &format!("{source}unknown = 1\n")).is_err());
        assert!(ProjectConfig::parse(&path, "version = 2").is_err());
        assert!(
            ProjectConfig::parse(&path, "version = 1")
                .unwrap()
                .profile(None)
                .is_none()
        );
        assert!(
            ProjectConfig::parse(
                &path,
                "version = 1\n[build.main]\ntarget = 'publish'\noutput = 'dist/out.md'"
            )
            .is_err()
        );
        fs::remove_dir_all(root).unwrap();
    }
}
