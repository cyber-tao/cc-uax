use crate::{ProjectLayout, ScanDiagnostic, ScanFailureStage};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

const GAME_MAPS_SETTINGS_SECTION: &str = "/Script/EngineSettings.GameMapsSettings";
/// Where the packaging settings that define what actually ships live.
const PACKAGING_SETTINGS_SECTION: &str = "/Script/UnrealEd.ProjectPackagingSettings";
/// How UE writes a null object reference in an `.ini` value.
const NULL_OBJECT_REFERENCE: &str = "None";
const ENTRY_POINT_KEYS: [&str; 7] = [
    "GameDefaultMap",
    "ServerDefaultMap",
    "EditorStartupMap",
    "TransitionMap",
    "GameInstanceClass",
    "GlobalDefaultGameMode",
    "GlobalDefaultServerGameMode",
];
/// `+MapsToCook=(FilePath="/Game/...")`: the maps a build actually ships.
const MAPS_TO_COOK_KEY: &str = "MapsToCook";
/// `+DirectoriesToAlwaysCook=(Path="/Game/...")`: a package-path prefix that
/// ships whether or not anything references it.
const DIRECTORIES_TO_COOK_KEY: &str = "DirectoriesToAlwaysCook";

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
    /// Packages a build ships regardless of what references them, from
    /// `ProjectPackagingSettings`. These are the project's real content roots:
    /// `GameDefaultMap` is often a developer map, and without the cook list the
    /// maps that actually ship look unreachable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cook_roots: Vec<ConfigReference>,
    /// Package-path prefixes from `+DirectoriesToAlwaysCook`. Every indexed
    /// package under one of these ships.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cook_directories: Vec<ConfigReference>,
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
    let mut cook = CookRoots::default();
    let config_root = layout.project_root().join("Config");

    for name in ["DefaultEngine.ini", "DefaultGame.ini"] {
        let path = config_root.join(name);
        let source = format!("Config/{name}");
        apply_if_regular_file(&path, &source, &mut defaults, &mut cook, &mut diagnostics);
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
        for name in candidates {
            let path = directory.join(&name);
            let source = format!("Config/{platform}/{name}");
            apply_if_regular_file(&path, &source, &mut effective, &mut cook, &mut diagnostics);
        }
        // Only record a platform when its config actually changes an entry point.
        // Most `Config/<Platform>/` directories hold packaging or SDK settings
        // only; treating their mere existence as an override reported platform
        // overrides that do not exist and listed every default root three times.
        if effective != defaults {
            platforms.insert(platform, effective);
        }
    }

    (
        ProjectEntryPoints {
            defaults,
            platforms,
            cook_roots: cook.maps.into_values().collect(),
            cook_directories: cook.directories.into_values().collect(),
        },
        diagnostics,
    )
}

/// Cook roots keyed by package path so a value repeated across ini layers is
/// recorded once.
#[derive(Debug, Default)]
struct CookRoots {
    maps: BTreeMap<String, ConfigReference>,
    directories: BTreeMap<String, ConfigReference>,
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
    cook: &mut CookRoots,
    diagnostics: &mut Vec<ScanDiagnostic>,
) {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            diagnostics.push(config_warning(
                source,
                format!("could not inspect config source: {error}"),
            ));
            return;
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        diagnostics.push(config_warning(
            source,
            "config source is not a regular project file",
        ));
        return;
    }
    apply_config_file(path, source, references, cook, diagnostics);
}

fn apply_config_file(
    path: &Path,
    source: &str,
    references: &mut BTreeMap<String, ConfigReference>,
    cook: &mut CookRoots,
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
    let mut section = Section::Other;
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
        if let Some(name) = parse_section(trimmed) {
            section = if name.eq_ignore_ascii_case(GAME_MAPS_SETTINGS_SECTION) {
                Section::GameMaps
            } else if name.eq_ignore_ascii_case(PACKAGING_SETTINGS_SECTION) {
                Section::Packaging
            } else {
                Section::Other
            };
            continue;
        }
        let Some((raw_key, raw_value)) = trimmed.split_once('=') else {
            continue;
        };
        if section == Section::Packaging {
            apply_cook_entry(raw_key.trim(), raw_value, source, cook);
            continue;
        }
        if section != Section::GameMaps {
            continue;
        }
        let Some(key) = canonical_entry_point_key(raw_key.trim()) else {
            continue;
        };

        match parse_config_object_path(raw_value) {
            ConfigValue::Reference {
                object_path,
                package_path,
            } => {
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
            // `Key=` and `Key=None` are how UE spells "no object", not errors.
            // Later files still win, so an explicit unset clears an earlier value.
            ConfigValue::Unset => {
                references.remove(key);
            }
            // A value that is neither a path nor an unset leaves whatever an
            // earlier file set: UE does not clear a configured property because a
            // later line failed to parse.
            ConfigValue::Invalid => diagnostics.push(config_warning(
                source,
                format!(
                    "invalid {key} entry-point object path at line {}",
                    line_index + 1
                ),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    GameMaps,
    Packaging,
    Other,
}

fn parse_section(line: &str) -> Option<&str> {
    line.strip_prefix('[')
        .and_then(|section| section.strip_suffix(']'))
        .map(str::trim)
}

/// `+MapsToCook=(FilePath="/Game/Maps/M")` and
/// `+DirectoriesToAlwaysCook=(Path="/Game/Dir")`.
///
/// Array keys carry a `+`/`-` prefix. UE treats `-` as a removal, so a removed
/// entry drops the recorded root rather than adding one.
fn apply_cook_entry(raw_key: &str, raw_value: &str, source: &str, cook: &mut CookRoots) {
    let (remove, key) = match raw_key.strip_prefix('-') {
        Some(rest) => (true, rest.trim()),
        None => (false, raw_key.trim_start_matches('+').trim()),
    };
    let (target, field) = if key.eq_ignore_ascii_case(MAPS_TO_COOK_KEY) {
        (&mut cook.maps, "FilePath")
    } else if key.eq_ignore_ascii_case(DIRECTORIES_TO_COOK_KEY) {
        (&mut cook.directories, "Path")
    } else {
        return;
    };
    let Some(package_path) = struct_field(raw_value, field) else {
        return;
    };
    if !is_valid_package_path(&package_path) {
        return;
    }
    if remove {
        target.remove(&package_path);
        return;
    }
    target.insert(
        package_path.clone(),
        ConfigReference {
            key: key.to_string(),
            source: source.to_string(),
            object_path: package_path.clone(),
            package_path,
        },
    );
}

/// Pulls one `Field="value"` out of an ini struct literal like
/// `(FilePath="/Game/Maps/M")`.
fn struct_field(value: &str, field: &str) -> Option<String> {
    let inner = value
        .trim()
        .strip_prefix('(')?
        .trim_end()
        .strip_suffix(')')?;
    for part in inner.split(',') {
        let Some((name, raw)) = part.split_once('=') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case(field) {
            let value = strip_matching_quotes(raw.trim()).trim();
            return (!value.is_empty()).then(|| value.to_string());
        }
    }
    None
}

fn canonical_entry_point_key(key: &str) -> Option<&'static str> {
    ENTRY_POINT_KEYS
        .iter()
        .copied()
        .find(|candidate| candidate.eq_ignore_ascii_case(key))
}

/// What a `GameMapsSettings` value says about its entry point. Keeping "no
/// object" apart from "not a path" matters: UE ships `TransitionMap=` and
/// `GlobalDefaultServerGameMode=None` in stock project configs, and reporting
/// those as invalid turns a healthy project into a `partial` report.
enum ConfigValue {
    Reference {
        object_path: String,
        package_path: String,
    },
    /// UE's spelling of a null object reference: an empty value or `None`.
    Unset,
    Invalid,
}

fn parse_config_object_path(value: &str) -> ConfigValue {
    let value = strip_matching_quotes(value.trim()).trim();
    if value.is_empty() || value.eq_ignore_ascii_case(NULL_OBJECT_REFERENCE) {
        return ConfigValue::Unset;
    }
    let object_path = match value.split_once('\'') {
        Some((class_name, quoted_path))
            if is_class_wrapper(class_name)
                && quoted_path.ends_with('\'')
                && !quoted_path[..quoted_path.len() - 1].contains('\'') =>
        {
            &quoted_path[..quoted_path.len() - 1]
        }
        Some(_) => return ConfigValue::Invalid,
        None => value,
    };
    let object_path = strip_matching_quotes(object_path.trim()).trim();
    if object_path.is_empty() || object_path.eq_ignore_ascii_case(NULL_OBJECT_REFERENCE) {
        return ConfigValue::Unset;
    }
    if !is_valid_object_path(object_path) {
        return ConfigValue::Invalid;
    }
    let package_path = object_path
        .split_once('.')
        .map(|(package, _)| package)
        .unwrap_or(object_path);
    if !is_valid_package_path(package_path) {
        return ConfigValue::Invalid;
    }
    ConfigValue::Reference {
        object_path: object_path.to_string(),
        package_path: package_path.to_string(),
    }
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
