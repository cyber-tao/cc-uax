use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectLayout {
    project_root: PathBuf,
    content_root: PathBuf,
    project_file: Option<PathBuf>,
    /// The sibling `.uproject` files that made the descriptor ambiguous, when
    /// more than one was found. Empty otherwise.
    #[serde(default)]
    ambiguous_project_files: Vec<PathBuf>,
}

/// A plugin content root as Unreal would mount it.
///
/// `FPluginManager` mounts `/{PluginName}/` where `PluginName` is the `.uplugin`
/// file's base name, *not* its directory name (`PluginManager.cpp`). Three of the
/// nine plugins in the reference corpus disagree on those two — `MetaXR` ships
/// `OculusXR.uplugin`, `LEJson` ships `LowEntryJson.uplugin`, `UAssetBrower5.3`
/// ships `UAssetBrowser.uplugin` — so guessing from the directory name produces
/// package paths that do not exist in the project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginContentRoot {
    pub package_root: String,
    pub content_dir: PathBuf,
}

impl ProjectLayout {
    pub fn discover(path: impl AsRef<Path>) -> Result<Self, ProjectLayoutError> {
        let input = canonicalize(path.as_ref(), "input path")?;
        if input.is_file() {
            if has_extension(&input, "uproject") {
                let root = input.parent().ok_or_else(|| {
                    ProjectLayoutError::Invalid(format!(
                        "project file has no parent directory: {}",
                        input.display()
                    ))
                })?;
                return Self::from_explicit_project_file(&input, root);
            }
            return Self::discover_from_ancestor(&input);
        }

        if input.is_dir() {
            if file_name_eq(&input, "Content") {
                return Self::from_content_root(&input);
            }
            if find_child_dir(&input, "Content")?.is_some() {
                return Self::from_project_root(&input);
            }
            return Self::discover_from_ancestor(&input);
        }

        Err(ProjectLayoutError::Invalid(format!(
            "input path is neither a file nor directory: {}",
            input.display()
        )))
    }

    /// Discover from an explicit `.uproject` path, even when sibling `.uproject`
    /// files exist (platform-specific targets sharing one Content tree).
    fn from_explicit_project_file(
        project_file: &Path,
        project_root: &Path,
    ) -> Result<Self, ProjectLayoutError> {
        let project_root = canonicalize(project_root, "project root")?;
        if !project_root.is_dir() {
            return Err(ProjectLayoutError::Invalid(format!(
                "project root is not a directory: {}",
                project_root.display()
            )));
        }
        let content_root = find_child_dir(&project_root, "Content")?.ok_or_else(|| {
            ProjectLayoutError::Invalid(format!(
                "project Content directory not found under {}",
                project_root.display()
            ))
        })?;
        let project_file = canonicalize(project_file, "project file")?;
        Ok(Self {
            project_root,
            content_root,
            project_file: Some(project_file),
            ambiguous_project_files: Vec::new(),
        })
    }

    pub fn from_project_root(path: impl AsRef<Path>) -> Result<Self, ProjectLayoutError> {
        let project_root = canonicalize(path.as_ref(), "project root")?;
        if !project_root.is_dir() {
            return Err(ProjectLayoutError::Invalid(format!(
                "project root is not a directory: {}",
                project_root.display()
            )));
        }
        let content_root = find_child_dir(&project_root, "Content")?.ok_or_else(|| {
            ProjectLayoutError::Invalid(format!(
                "project Content directory not found under {}",
                project_root.display()
            ))
        })?;
        Self::finish(project_root, content_root)
    }

    pub fn from_content_root(path: impl AsRef<Path>) -> Result<Self, ProjectLayoutError> {
        let content_root = canonicalize(path.as_ref(), "Content root")?;
        if !content_root.is_dir() {
            return Err(ProjectLayoutError::Invalid(format!(
                "Content root is not a directory: {}",
                content_root.display()
            )));
        }
        if !file_name_eq(&content_root, "Content") {
            return Err(ProjectLayoutError::Invalid(format!(
                "expected a directory named Content, got {}",
                content_root.display()
            )));
        }
        let project_root = content_root.parent().ok_or_else(|| {
            ProjectLayoutError::Invalid(format!(
                "Content directory has no project parent: {}",
                content_root.display()
            ))
        })?;
        Self::finish(project_root.to_path_buf(), content_root)
    }

    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    pub fn content_root(&self) -> &Path {
        &self.content_root
    }

    pub fn project_file(&self) -> Option<&Path> {
        self.project_file.as_deref()
    }

    /// The sibling `.uproject` files that left the descriptor unresolved, if any.
    pub fn ambiguous_project_files(&self) -> &[PathBuf] {
        &self.ambiguous_project_files
    }

    /// Plugin content roots under `Plugins/`, mounted the way Unreal mounts them.
    ///
    /// Without these, plugin packages are invisible to inventory, adjacency,
    /// reachability and World Partition ownership even though the project loads
    /// them: the reference corpus hides 254, 109 and 60 assets plus four maps
    /// behind them. A plugin directory with no `Content` is skipped, since it
    /// cannot contribute packages.
    pub fn plugin_content_roots(&self) -> Vec<PluginContentRoot> {
        let mut roots = Vec::new();
        let plugins_dir = self.project_root.join("Plugins");
        if !plugins_dir.is_dir() {
            return roots;
        }
        collect_plugin_roots(&plugins_dir, &mut roots, 0);
        roots.sort_by(|left, right| left.package_root.cmp(&right.package_root));
        roots.dedup_by(|left, right| left.package_root == right.package_root);
        roots
    }

    fn discover_from_ancestor(path: &Path) -> Result<Self, ProjectLayoutError> {
        for ancestor in path.ancestors() {
            if file_name_eq(ancestor, "Content") {
                return Self::from_content_root(ancestor);
            }
        }
        Err(ProjectLayoutError::Invalid(format!(
            "could not locate a project root or Content ancestor for {}",
            path.display()
        )))
    }

    fn finish(project_root: PathBuf, content_root: PathBuf) -> Result<Self, ProjectLayoutError> {
        let mut project_files = fs::read_dir(&project_root)
            .map_err(|source| ProjectLayoutError::Io {
                context: format!("read project root {}", project_root.display()),
                source,
            })?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_file() && has_extension(path, "uproject"))
            .collect::<Vec<_>>();
        project_files.sort_by_key(|path| normalized_path(path));
        // Which Content tree to scan is not in doubt — the caller named it, or it
        // is the one Content directory under this root. Only which descriptor
        // supplies entry points and cook roots is, and platform variants sharing
        // one Content tree (`Game.uproject`, `Game_Steam.uproject`, ...) are a
        // normal layout. Failing the whole scan over that made those projects
        // unscannable by their Content path, so the ambiguity is reported as
        // missing entry-point evidence instead. Naming a `.uproject` explicitly
        // still resolves it.
        if project_files.len() > 1 {
            return Ok(Self {
                project_root,
                content_root,
                project_file: None,
                ambiguous_project_files: project_files,
            });
        }
        Ok(Self {
            project_root,
            content_root,
            project_file: project_files.pop(),
            ambiguous_project_files: Vec::new(),
        })
    }
}

#[derive(Debug)]
pub enum ProjectLayoutError {
    Invalid(String),
    Io {
        context: String,
        source: std::io::Error,
    },
}

impl fmt::Display for ProjectLayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => f.write_str(message),
            Self::Io { context, source } => write!(f, "{context}: {source}"),
        }
    }
}

impl std::error::Error for ProjectLayoutError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Invalid(_) => None,
        }
    }
}

/// Plugins nest (`Plugins/NVIDIA/DLSS/DLSS.uplugin` in the reference corpus), so
/// the walk recurses, but a plugin never contains another plugin's content root
/// so it stops descending once it finds a `.uplugin`.
fn collect_plugin_roots(dir: &Path, roots: &mut Vec<PluginContentRoot>, depth: u32) {
    const MAX_PLUGIN_NESTING: u32 = 8;
    if depth > MAX_PLUGIN_NESTING {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut subdirs = Vec::new();
    let mut plugin_name = None;
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            subdirs.push(path);
        } else if has_extension(&path, "uplugin") {
            let name = path
                .file_stem()
                .and_then(|value| value.to_str())
                .map(str::to_owned);
            // Deterministic when a directory somehow holds two descriptors.
            plugin_name = match (plugin_name, name) {
                (Some(current), Some(candidate)) if candidate < current => Some(candidate),
                (Some(current), _) => Some(current),
                (None, candidate) => candidate,
            };
        }
    }
    if let Some(name) = plugin_name {
        if let Some(content_dir) = subdirs.iter().find(|path| file_name_eq(path, "Content")) {
            roots.push(PluginContentRoot {
                package_root: format!("/{name}"),
                content_dir: content_dir.clone(),
            });
        }
        return;
    }
    subdirs.sort_by_key(|path| normalized_path(path));
    for subdir in subdirs {
        collect_plugin_roots(&subdir, roots, depth + 1);
    }
}

fn canonicalize(path: &Path, label: &str) -> Result<PathBuf, ProjectLayoutError> {
    fs::canonicalize(path).map_err(|source| ProjectLayoutError::Io {
        context: format!("locate {label} {}", path.display()),
        source,
    })
}

fn find_child_dir(parent: &Path, name: &str) -> Result<Option<PathBuf>, ProjectLayoutError> {
    let mut matches = fs::read_dir(parent)
        .map_err(|source| ProjectLayoutError::Io {
            context: format!("read directory {}", parent.display()),
            source,
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && file_name_eq(path, name))
        .collect::<Vec<_>>();
    matches.sort_by_key(|path| normalized_path(path));
    Ok(matches.into_iter().next())
}

fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(extension))
}

fn file_name_eq(path: &Path, expected: &str) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(expected))
}

fn normalized_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
}
