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
    /// `/Game` plus every plugin content root Unreal would mount.
    ///
    /// Leaving plugin roots out made their packages invisible to inventory,
    /// adjacency and reachability, and asking users to add them by hand is worse
    /// than doing it here: the mount name is the `.uplugin` base name, which
    /// routinely differs from the directory name, so a hand-written `--mount`
    /// silently invents package paths the project does not contain.
    pub fn default_for(layout: &ProjectLayout) -> Self {
        let mut mounts = vec![MountSpec {
            package_root: "/Game".to_string(),
            disk_root: layout.content_root().to_path_buf(),
        }];
        for plugin in layout.plugin_content_roots() {
            mounts.push(MountSpec {
                package_root: plugin.package_root,
                disk_root: plugin.content_dir,
            });
        }
        Self { mounts }
    }

    pub fn parse(layout: &ProjectLayout, value: &str) -> Result<Self, MountTableError> {
        let mut mounts = Vec::new();
        for raw in value.split(',') {
            if let Some(spec) = parse_mount_token(layout, raw)? {
                mounts.push(spec);
            }
        }
        if mounts.is_empty() {
            return Err(MountTableError::Invalid(
                "mount table must contain at least one mapping".to_string(),
            ));
        }
        Ok(Self { mounts })
    }

    /// The default mounts with `requested` applied on top.
    ///
    /// Explicit mounts add to the auto-discovered set rather than replacing it,
    /// because a user naming one plugin root should not silently drop `/Game` or
    /// the project's other plugins. A request for a root that already exists
    /// replaces it, which is how a caller redirects a mount deliberately.
    pub fn resolve(layout: &ProjectLayout, requested: &[String]) -> Result<Self, MountTableError> {
        let mut mounts = Self::default_for(layout).mounts;
        for raw in requested.iter().flat_map(|value| value.split(',')) {
            let Some(spec) = parse_mount_token(layout, raw)? else {
                continue;
            };
            match mounts.iter_mut().find(|existing| {
                existing
                    .package_root
                    .eq_ignore_ascii_case(&spec.package_root)
            }) {
                Some(existing) => *existing = spec,
                None => mounts.push(spec),
            }
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

/// One `PACKAGE_ROOT=RELATIVE_DIR` token, or `None` when the token is blank.
fn parse_mount_token(
    layout: &ProjectLayout,
    raw: &str,
) -> Result<Option<MountSpec>, MountTableError> {
    let token = raw.trim();
    if token.is_empty() {
        return Ok(None);
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
        None if token.eq_ignore_ascii_case("/Game") => (token, layout.content_root().to_path_buf()),
        None => {
            return Err(MountTableError::Invalid(format!(
                "mount '{token}' needs a project-relative disk path, for example /Plugin=Plugins/X/Content"
            )));
        }
    };
    MountSpec::new(package_root, disk_path).map(Some)
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

/// Strip a trailing `.uasset` or `.umap` extension (case-insensitive) if present.
///
/// Shared by mount-path normalization, project ownership matching, and CLI
/// `--focus` pattern handling so the asset-extension rule lives in one place.
pub fn strip_asset_extension(path: &str) -> &str {
    for ext in [".uasset", ".umap"] {
        if let Some(idx) = path.len().checked_sub(ext.len())
            && path
                .get(idx..)
                .is_some_and(|tail| tail.eq_ignore_ascii_case(ext))
        {
            return &path[..idx];
        }
    }
    path
}
