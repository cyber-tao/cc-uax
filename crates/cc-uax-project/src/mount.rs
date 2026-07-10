use crate::ProjectLayout;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MountSpec {
    package_root: String,
    disk_root: PathBuf,
}

impl MountSpec {
    pub fn new(
        package_root: impl AsRef<str>,
        disk_root: impl AsRef<Path>,
    ) -> Result<Self, MountTableError> {
        let package_root = normalize_package_root(package_root.as_ref())?;
        let disk_root =
            fs::canonicalize(disk_root.as_ref()).map_err(|source| MountTableError::Io {
                context: format!("locate mount root {}", disk_root.as_ref().display()),
                source,
            })?;
        if !disk_root.is_dir() {
            return Err(MountTableError::Invalid(format!(
                "mount disk root is not a directory: {}",
                disk_root.display()
            )));
        }
        Ok(Self {
            package_root,
            disk_root,
        })
    }

    pub fn package_root(&self) -> &str {
        &self.package_root
    }

    pub fn disk_root(&self) -> &Path {
        &self.disk_root
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MountTable {
    mounts: Vec<MountSpec>,
}

impl MountTable {
    pub fn default_for(layout: &ProjectLayout) -> Self {
        Self {
            mounts: vec![MountSpec {
                package_root: "/Game".to_string(),
                disk_root: layout.content_root().to_path_buf(),
            }],
        }
    }

    pub fn parse(layout: &ProjectLayout, value: &str) -> Result<Self, MountTableError> {
        let mut mounts = Vec::new();
        for raw in value.split(',') {
            let token = raw.trim();
            if token.is_empty() {
                continue;
            }
            let (package_root, disk_path) = match token.split_once('=') {
                Some((package_root, disk_path)) if !disk_path.trim().is_empty() => {
                    (package_root, layout.project_root().join(disk_path.trim()))
                }
                Some((_, _)) => {
                    return Err(MountTableError::Invalid(format!(
                        "mount disk path is empty in '{token}'"
                    )));
                }
                None if token.eq_ignore_ascii_case("/Game") => {
                    (token, layout.content_root().to_path_buf())
                }
                None => {
                    return Err(MountTableError::Invalid(format!(
                        "mount '{token}' needs a project-relative disk path, for example /Plugin=Plugins/X/Content"
                    )));
                }
            };
            mounts.push(MountSpec::new(package_root, disk_path)?);
        }
        if mounts.is_empty() {
            return Err(MountTableError::Invalid(
                "mount table must contain at least one mapping".to_string(),
            ));
        }
        Ok(Self { mounts })
    }

    pub fn new(mounts: Vec<MountSpec>) -> Result<Self, MountTableError> {
        if mounts.is_empty() {
            return Err(MountTableError::Invalid(
                "mount table must contain at least one mapping".to_string(),
            ));
        }
        Ok(Self { mounts })
    }

    pub fn mounts(&self) -> &[MountSpec] {
        &self.mounts
    }
}

pub fn package_path_from_relative(
    relative_path: &str,
    package_root: &str,
) -> Result<String, MountTableError> {
    let package_root = normalize_package_root(package_root)?;
    let normalized = relative_path.replace('\\', "/");
    let normalized = normalized.trim_matches('/');
    let package_relative = strip_asset_extension(normalized);
    if package_relative.is_empty() {
        return Err(MountTableError::Invalid(
            "asset path must not be empty".to_string(),
        ));
    }
    Ok(format!("{}/{}", package_root, package_relative))
}

#[derive(Debug)]
pub enum MountTableError {
    Invalid(String),
    Io {
        context: String,
        source: std::io::Error,
    },
}

impl fmt::Display for MountTableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => f.write_str(message),
            Self::Io { context, source } => write!(f, "{context}: {source}"),
        }
    }
}

impl std::error::Error for MountTableError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Invalid(_) => None,
        }
    }
}

fn normalize_package_root(value: &str) -> Result<String, MountTableError> {
    let value = value.trim();
    if value.trim_matches('/').is_empty() {
        return Err(MountTableError::Invalid(
            "mount package root must not be empty".to_string(),
        ));
    }
    if value.contains([':', '\\']) || value.contains(char::is_whitespace) {
        return Err(MountTableError::Invalid(format!(
            "mount package root '{value}' must look like /Game or /Plugin"
        )));
    }
    Ok(format!("/{}", value.trim_matches('/')))
}

fn strip_asset_extension(path: &str) -> &str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".uasset") {
        &path[..path.len() - 7]
    } else if lower.ends_with(".umap") {
        &path[..path.len() - 5]
    } else {
        path
    }
}
