use crate::{ProjectLayout, ScanDiagnostic, ScanFailureStage};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

const GAME_MAPS_SETTINGS_SECTION: &str = "/Script/EngineSettings.GameMapsSettings";
const ENTRY_POINT_KEYS: [&str; 7] = [
    "GameDefaultMap",
    "ServerDefaultMap",
    "EditorStartupMap",
    "TransitionMap",
    "GameInstanceClass",
    "GlobalDefaultGameMode",
    "GlobalDefaultServerGameMode",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigReference {
    pub key: String,
    pub source: String,
    pub object_path: String,
    pub package_path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectEntryPoints {
    pub defaults: BTreeMap<String, ConfigReference>,
    pub platforms: BTreeMap<String, BTreeMap<String, ConfigReference>>,
}

impl ProjectEntryPoints {
    pub fn reference(&self, key: &str) -> Option<&ConfigReference> {
        lookup_reference(&self.defaults, key)
    }

    pub fn reference_for_platform(&self, platform: &str, key: &str) -> Option<&ConfigReference> {
        if let Some((_, references)) = self
            .platforms
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(platform))
        {
            return lookup_reference(references, key);
        }
        self.reference(key)
    }
}

pub(crate) fn load_project_entry_points(
    layout: &ProjectLayout,
) -> (ProjectEntryPoints, Vec<ScanDiagnostic>) {
    let mut diagnostics = Vec::new();
    let mut defaults = BTreeMap::new();
    let config_root = layout.project_root().join("Config");

    for name in ["DefaultEngine.ini", "DefaultGame.ini"] {
        let path = config_root.join(name);
        let source = format!("Config/{name}");
        apply_if_regular_file(&path, &source, &mut defaults, &mut diagnostics);
    }

    let platform_directories = discover_platform_directories(&config_root, &mut diagnostics);
    let mut platforms = BTreeMap::new();
    for (platform, directory) in platform_directories {
        let mut effective = defaults.clone();
        let mut candidates = BTreeSet::new();
        candidates.insert("DefaultEngine.ini".to_string());
        candidates.insert("DefaultGame.ini".to_string());
        candidates.insert(format!("{platform}Engine.ini"));
        candidates.insert(format!("{platform}Game.ini"));
        let mut found = false;
        for name in candidates {
            let path = directory.join(&name);
            let source = format!("Config/{platform}/{name}");
            found |= apply_if_regular_file(&path, &source, &mut effective, &mut diagnostics);
        }
        if found {
            platforms.insert(platform, effective);
        }
    }

    (
        ProjectEntryPoints {
            defaults,
            platforms,
        },
        diagnostics,
    )
}

fn discover_platform_directories(
    config_root: &Path,
    diagnostics: &mut Vec<ScanDiagnostic>,
) -> Vec<(String, PathBuf)> {
    let metadata = match fs::symlink_metadata(config_root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => {
            diagnostics.push(config_warning(
                "Config",
                format!("could not inspect config directory: {error}"),
            ));
            return Vec::new();
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        diagnostics.push(config_warning(
            "Config",
            "config directory is not a regular project directory",
        ));
        return Vec::new();
    }

    let entries = match fs::read_dir(config_root) {
        Ok(entries) => entries,
        Err(error) => {
            diagnostics.push(config_warning(
                "Config",
                format!("could not enumerate config directory: {error}"),
            ));
            return Vec::new();
        }
    };
    let mut platforms = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                diagnostics.push(config_warning(
                    "Config",
                    format!("could not enumerate a config entry: {error}"),
                ));
                continue;
            }
        };
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                diagnostics.push(config_warning(
                    "Config",
                    format!("could not inspect a config entry: {error}"),
                ));
                continue;
            }
        };
        if file_type.is_symlink() || !file_type.is_dir() {
            continue;
        }
        let Some(platform) = entry.file_name().to_str().map(str::to_owned) else {
            diagnostics.push(config_warning(
                "Config",
                "ignored a platform config directory with a non-Unicode name",
            ));
            continue;
        };
        platforms.push((platform, entry.path()));
    }
    platforms.sort_by(|left, right| {
        left.0
            .to_ascii_lowercase()
            .cmp(&right.0.to_ascii_lowercase())
    });
    platforms
}

fn apply_if_regular_file(
    path: &Path,
    source: &str,
    references: &mut BTreeMap<String, ConfigReference>,
    diagnostics: &mut Vec<ScanDiagnostic>,
) -> bool {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
        Err(error) => {
            diagnostics.push(config_warning(
                source,
                format!("could not inspect config source: {error}"),
            ));
            return true;
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        diagnostics.push(config_warning(
            source,
            "config source is not a regular project file",
        ));
        return true;
    }
    apply_config_file(path, source, references, diagnostics);
    true
}

fn apply_config_file(
    path: &Path,
    source: &str,
    references: &mut BTreeMap<String, ConfigReference>,
    diagnostics: &mut Vec<ScanDiagnostic>,
) {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) => {
            diagnostics.push(config_warning(
                source,
                format!("could not read config source: {error}"),
            ));
            return;
        }
    };
    let mut in_entry_point_section = false;
    for (line_index, line) in BufReader::new(file).lines().enumerate() {
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                diagnostics.push(config_warning(
                    source,
                    format!(
                        "could not decode config source at line {}: {error}",
                        line_index + 1
                    ),
                ));
                return;
            }
        };
        let trimmed = line.trim().trim_start_matches('\u{feff}');
        if trimmed.is_empty() || trimmed.starts_with(';') || trimmed.starts_with('#') {
            continue;
        }
        if let Some(section) = parse_section(trimmed) {
            in_entry_point_section = section.eq_ignore_ascii_case(GAME_MAPS_SETTINGS_SECTION);
            continue;
        }
        if !in_entry_point_section {
            continue;
        }
        let Some((raw_key, raw_value)) = trimmed.split_once('=') else {
            continue;
        };
        let Some(key) = canonical_entry_point_key(raw_key.trim()) else {
            continue;
        };

        references.remove(key);
        match parse_config_object_path(raw_value) {
            Some((object_path, package_path)) => {
                references.insert(
                    key.to_string(),
                    ConfigReference {
                        key: key.to_string(),
                        source: source.to_string(),
                        object_path,
                        package_path,
                    },
                );
            }
            None => diagnostics.push(config_warning(
                source,
                format!(
                    "invalid {key} entry-point object path at line {}",
                    line_index + 1
                ),
            )),
        }
    }
}

fn parse_section(line: &str) -> Option<&str> {
    line.strip_prefix('[')
        .and_then(|section| section.strip_suffix(']'))
        .map(str::trim)
}

fn canonical_entry_point_key(key: &str) -> Option<&'static str> {
    ENTRY_POINT_KEYS
        .iter()
        .copied()
        .find(|candidate| candidate.eq_ignore_ascii_case(key))
}

fn parse_config_object_path(value: &str) -> Option<(String, String)> {
    let value = strip_matching_quotes(value.trim());
    let object_path = match value.split_once('\'') {
        Some((class_name, quoted_path))
            if is_class_wrapper(class_name)
                && quoted_path.ends_with('\'')
                && !quoted_path[..quoted_path.len() - 1].contains('\'') =>
        {
            &quoted_path[..quoted_path.len() - 1]
        }
        Some(_) => return None,
        None => value,
    };
    let object_path = strip_matching_quotes(object_path.trim());
    if !is_valid_object_path(object_path) {
        return None;
    }
    let package_path = object_path
        .split_once('.')
        .map(|(package, _)| package)
        .unwrap_or(object_path);
    if !is_valid_package_path(package_path) {
        return None;
    }
    Some((object_path.to_string(), package_path.to_string()))
}

fn strip_matching_quotes(value: &str) -> &str {
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        &value[1..value.len() - 1]
    } else {
        value
    }
}

fn is_class_wrapper(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn is_valid_object_path(value: &str) -> bool {
    if value.is_empty()
        || value.contains(['\\', ':', '\r', '\n', '\t'])
        || value.chars().any(char::is_whitespace)
    {
        return false;
    }
    let mut parts = value.split('.');
    let Some(package) = parts.next() else {
        return false;
    };
    if !is_valid_package_path(package) {
        return false;
    }
    let Some(object) = parts.next() else {
        return true;
    };
    !object.is_empty() && parts.next().is_none()
}

fn is_valid_package_path(value: &str) -> bool {
    value.starts_with('/')
        && !value.starts_with("//")
        && value.len() > 1
        && value
            .split('/')
            .skip(1)
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

fn lookup_reference<'a>(
    references: &'a BTreeMap<String, ConfigReference>,
    key: &str,
) -> Option<&'a ConfigReference> {
    references
        .iter()
        .find(|(candidate, _)| candidate.eq_ignore_ascii_case(key))
        .map(|(_, reference)| reference)
}

fn config_warning(path: impl Into<PathBuf>, message: impl Into<String>) -> ScanDiagnostic {
    ScanDiagnostic::warning(path, ScanFailureStage::Config, message)
}
