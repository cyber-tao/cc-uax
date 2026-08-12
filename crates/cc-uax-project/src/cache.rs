use crate::{AssetAnalysisSummary, ProjectLayout};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

const CACHE_NAMESPACE: &str = "cc-uax/projects";
// A fixed name (no embedded version) so a `user_version`/`tool_version` mismatch
// on the same file actually triggers the in-place reset, instead of silently
// orphaning a version-named file on every schema change.
const CACHE_FILE_NAME: &str = "project-index.sqlite";
const CACHE_SCHEMA_VERSION: i64 = 2;
// Cached analysis is only valid for the binary that produced it: a tool upgrade
// can change decoded values without changing a file's mtime/size. Bind cache
// validity to the crate version so a new release invalidates stale analysis.
const CACHE_TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "policy", content = "path", rename_all = "snake_case")]
pub enum CachePathPolicy {
    Disabled,
    #[default]
    System,
    CustomFile(PathBuf),
}

impl CachePathPolicy {
    pub fn resolve(&self, project: &ProjectLayout) -> Result<Option<PathBuf>, CachePathError> {
        match self {
            Self::Disabled => Ok(None),
            Self::CustomFile(path) => Ok(Some(path.clone())),
            Self::System => {
                let root = system_cache_root()?.join(CACHE_NAMESPACE);
                let project_key = project_cache_key(project.project_root());
                Ok(Some(root.join(project_key).join(CACHE_FILE_NAME)))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachePathError {
    message: String,
}

impl fmt::Display for CachePathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CachePathError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CacheEntry {
    pub(crate) mtime: i64,
    pub(crate) size: i64,
    pub(crate) parse_ok: bool,
    pub(crate) references: Vec<String>,
    pub(crate) analysis: Option<AssetAnalysisSummary>,
    pub(crate) parse_error: Option<String>,
}

/// Sentinel mtime used when a file's modification time cannot be read reliably.
/// An entry compared against an unknown mtime is never fresh, forcing a re-parse.
pub(crate) const UNKNOWN_MTIME: i64 = i64::MIN;

impl CacheEntry {
    pub(crate) fn is_fresh(&self, mtime: i64, size: i64) -> bool {
        mtime != UNKNOWN_MTIME
            && self.mtime != UNKNOWN_MTIME
            && self.mtime == mtime
            && self.size == size
    }
}

pub(crate) struct ProjectCache {
    connection: Connection,
    loaded: HashMap<String, CacheEntry>,
    reset_reason: Option<String>,
}

impl ProjectCache {
    pub(crate) fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)
                .map_err(|error| format!("create cache directory {}: {error}", parent.display()))?;
        }
        let connection = Connection::open(path)
            .map_err(|error| format!("open cache database {}: {error}", path.display()))?;
        let mut reset_reason: Option<String> = None;
        let version = connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .map_err(|error| format!("read cache schema version: {error}"))?;
        if version != CACHE_SCHEMA_VERSION {
            connection
                .execute("DROP TABLE IF EXISTS package_refs", [])
                .map_err(|error| format!("reset cache table: {error}"))?;
            connection
                .pragma_update(None, "user_version", CACHE_SCHEMA_VERSION)
                .map_err(|error| format!("set cache schema version: {error}"))?;
            // Version 0 is a brand-new database, not a discarded prior cache.
            if version != 0 {
                reset_reason = Some(format!(
                    "cache schema version changed from {version} to {CACHE_SCHEMA_VERSION}; cached analysis was discarded"
                ));
            }
        }
        connection
            .execute(
                "CREATE TABLE IF NOT EXISTS package_refs (
                    file_path   TEXT PRIMARY KEY,
                    mtime       INTEGER NOT NULL,
                    size        INTEGER NOT NULL,
                    parse_ok    INTEGER NOT NULL,
                    refs        TEXT NOT NULL,
                    analysis    TEXT,
                    parse_error TEXT
                )",
                [],
            )
            .map_err(|error| format!("create cache table: {error}"))?;
        connection
            .execute(
                "CREATE TABLE IF NOT EXISTS cache_meta (
                    key   TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                )",
                [],
            )
            .map_err(|error| format!("create cache meta table: {error}"))?;
        let cached_tool_version: Option<String> = connection
            .query_row(
                "SELECT value FROM cache_meta WHERE key = 'tool_version'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| format!("read cache tool version: {error}"))?;
        if cached_tool_version.as_deref() != Some(CACHE_TOOL_VERSION) {
            connection
                .execute("DELETE FROM package_refs", [])
                .map_err(|error| format!("reset cache for new tool version: {error}"))?;
            connection
                .execute(
                    "INSERT OR REPLACE INTO cache_meta (key, value) VALUES ('tool_version', ?1)",
                    params![CACHE_TOOL_VERSION],
                )
                .map_err(|error| format!("record cache tool version: {error}"))?;
            if cached_tool_version.is_some() {
                reset_reason.get_or_insert_with(|| {
                    format!(
                        "cache tool version changed to {CACHE_TOOL_VERSION}; cached analysis was discarded"
                    )
                });
            }
        }

        let mut loaded = HashMap::new();
        {
            let mut statement = connection
                .prepare(
                    "SELECT file_path, mtime, size, parse_ok, refs, analysis, parse_error FROM package_refs",
                )
                .map_err(|error| format!("prepare cache load: {error}"))?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        CacheEntry {
                            mtime: row.get(1)?,
                            size: row.get(2)?,
                            parse_ok: row.get::<_, i64>(3)? != 0,
                            references: split_references(&row.get::<_, String>(4)?),
                            analysis: None,
                            parse_error: row.get(6)?,
                        },
                        row.get::<_, Option<String>>(5)?,
                    ))
                })
                .map_err(|error| format!("query cache entries: {error}"))?;
            for row in rows {
                let (path, mut entry, analysis) =
                    row.map_err(|error| format!("decode cache entry: {error}"))?;
                if let Some(analysis) = analysis {
                    entry.analysis =
                        Some(serde_json::from_str(&analysis).map_err(|error| {
                            format!("decode cache analysis for {path}: {error}")
                        })?);
                }
                loaded.insert(path, entry);
            }
        }
        Ok(Self {
            connection,
            loaded,
            reset_reason,
        })
    }

    /// Reason the on-disk cache was cleared while opening (schema or tool-version
    /// change), if any.
    pub(crate) fn reset_reason(&self) -> Option<&str> {
        self.reset_reason.as_deref()
    }

    pub(crate) fn lookup(&self, key: &str, mtime: i64, size: i64) -> Option<&CacheEntry> {
        self.loaded
            .get(key)
            .filter(|entry| entry.is_fresh(mtime, size))
    }

    pub(crate) fn store(&mut self, current: &HashMap<String, CacheEntry>) -> Result<bool, String> {
        if self.loaded == *current {
            return Ok(false);
        }
        let transaction = self
            .connection
            .transaction()
            .map_err(|error| format!("start cache transaction: {error}"))?;
        {
            let mut delete = transaction
                .prepare("DELETE FROM package_refs WHERE file_path = ?1")
                .map_err(|error| format!("prepare cache delete: {error}"))?;
            for path in self.loaded.keys() {
                if !current.contains_key(path) {
                    delete
                        .execute(params![path])
                        .map_err(|error| format!("delete cache entry for {path}: {error}"))?;
                }
            }
            let mut upsert = transaction
                .prepare(
                    "INSERT OR REPLACE INTO package_refs
                     (file_path, mtime, size, parse_ok, refs, analysis, parse_error)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                )
                .map_err(|error| format!("prepare cache store: {error}"))?;
            for (path, entry) in current {
                // Only rewrite rows that are new or actually changed.
                if self.loaded.get(path) == Some(entry) {
                    continue;
                }
                let analysis = entry
                    .analysis
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()
                    .map_err(|error| format!("encode cache analysis for {path}: {error}"))?;
                upsert
                    .execute(params![
                        path,
                        entry.mtime,
                        entry.size,
                        entry.parse_ok as i64,
                        join_references(&entry.references),
                        analysis.as_deref(),
                        entry.parse_error.as_deref(),
                    ])
                    .map_err(|error| format!("store cache entry for {path}: {error}"))?;
            }
        }
        transaction
            .commit()
            .map_err(|error| format!("commit cache transaction: {error}"))?;
        self.loaded = current.clone();
        Ok(true)
    }
}

fn system_cache_root() -> Result<PathBuf, CachePathError> {
    if cfg!(target_os = "windows") {
        return env_path("LOCALAPPDATA").ok_or_else(|| unavailable("LOCALAPPDATA"));
    }
    if cfg!(target_os = "macos") {
        return env_path("HOME")
            .map(|home| home.join("Library/Caches"))
            .ok_or_else(|| unavailable("HOME"));
    }
    if let Some(root) = env_path("XDG_CACHE_HOME") {
        return Ok(root);
    }
    env_path("HOME")
        .map(|home| home.join(".cache"))
        .ok_or_else(|| unavailable("XDG_CACHE_HOME or HOME"))
}

fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn unavailable(variable: &str) -> CachePathError {
    CachePathError {
        message: format!("system cache directory unavailable: {variable} is not set"),
    }
}

fn project_cache_key(project_root: &Path) -> String {
    let normalized = project_root.to_string_lossy().replace('\\', "/");
    let normalized = if cfg!(target_os = "windows") {
        normalized.to_ascii_lowercase()
    } else {
        normalized
    };
    let mut hash = 0xcbf29ce484222325u64;
    for byte in normalized.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    let name = project_root
        .file_name()
        .and_then(|value| value.to_str())
        .map(sanitize_component)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "project".to_string());
    format!("{name}-{hash:016x}")
}

fn sanitize_component(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn join_references(references: &[String]) -> String {
    references.join("\n")
}

fn split_references(value: &str) -> Vec<String> {
    if value.is_empty() {
        Vec::new()
    } else {
        value.split('\n').map(str::to_owned).collect()
    }
}
