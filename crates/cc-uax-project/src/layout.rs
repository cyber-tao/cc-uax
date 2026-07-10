use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectLayout {
    project_root: PathBuf,
    content_root: PathBuf,
    project_file: Option<PathBuf>,
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
                return Self::from_project_root(root);
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
        if project_files.len() > 1 {
            return Err(ProjectLayoutError::Invalid(format!(
                "multiple .uproject files found under {}: {}",
                project_root.display(),
                project_files
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        Ok(Self {
            project_root,
            content_root,
            project_file: project_files.pop(),
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
