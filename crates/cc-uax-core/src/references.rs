//! Package-reference extraction from parsed import tables.
//!
//! Filesystem mounts, project scanning, reverse adjacency, and cache policy live
//! in `cc-uax-project`; the parser core only classifies references already
//! present in one package.

use crate::graph_models::LogicGraph;
use crate::model::{
    AssetExport, AssetReferences, DecodedValue, ReferenceEvidence, ReferenceEvidenceSources,
};
use crate::package::Package;
use std::collections::BTreeSet;

const PACKAGE_CLASS_NAME: &str = "Package";
const SCRIPT_PATH_PREFIX: &str = "/Script/";

/// Key an `FSoftObjectPath`-style string carries its package under, as written by
/// the property decoder for a soft object or class reference.
const ASSET_PATH_KEY: &str = "asset_path";

/// Bytecode reference kind that names a function rather than a path, so it can
/// never contribute a package.
const BYTECODE_FUNCTION_NAME_KIND: &str = "function_name";

impl Package {
    pub(crate) fn import_class_object_names(&self) -> impl Iterator<Item = (String, String)> + '_ {
        self.imports.iter().map(|import| {
            (
                self.names.resolve_raw(import.class_name),
                self.names.resolve_raw(import.object_name),
            )
        })
    }
}

pub(crate) fn collect_package_references<I, S>(imports: I) -> (Vec<String>, Vec<String>)
where
    I: IntoIterator<Item = (S, S)>,
    S: AsRef<str>,
{
    let mut assets = BTreeSet::new();
    let mut scripts = BTreeSet::new();
    for (class, name) in imports {
        if class.as_ref() != PACKAGE_CLASS_NAME {
            continue;
        }
        let name = name.as_ref();
        if name.is_empty() || name == "None" {
            continue;
        }
        if name.starts_with(SCRIPT_PATH_PREFIX) {
            scripts.insert(name.to_owned());
        } else {
            assets.insert(name.to_owned());
        }
    }
    (assets.into_iter().collect(), scripts.into_iter().collect())
}

/// Reduce an object path to the package that owns it: `/Game/A/B.B:Sub` becomes
/// `/Game/A/B`. Returns `None` for anything that is not a mount-rooted path.
pub fn package_path_from_object_path(path: &str) -> Option<String> {
    let path = path.trim();
    if path.is_empty() || path.eq_ignore_ascii_case("None") || !path.starts_with('/') {
        return None;
    }
    let package = path
        .split_once('.')
        .map(|(package, _)| package)
        .unwrap_or(path);
    (!package.is_empty()).then(|| package.to_string())
}

/// Collect every package path reachable from one decoded value, walking arrays
/// and objects.
///
/// A plain string is treated as a candidate path because that is how a
/// string-literal asset reference is serialized; an object contributes its
/// `asset_path` (an `FSoftObjectPath`) before its members are walked.
pub fn collect_package_paths_from_value(value: &DecodedValue, out: &mut BTreeSet<String>) {
    if let Some(path) = value.as_str() {
        if let Some(package) = package_path_from_object_path(path) {
            out.insert(package);
        }
        return;
    }
    if let Some(object) = value.as_object() {
        if let Some(path) = object.get(ASSET_PATH_KEY).and_then(DecodedValue::as_str)
            && let Some(package) = package_path_from_object_path(path)
        {
            out.insert(package);
        }
        for nested in object.values() {
            collect_package_paths_from_value(nested, out);
        }
        return;
    }
    if let Some(array) = value.as_array() {
        for nested in array {
            collect_package_paths_from_value(nested, out);
        }
    }
}

/// Characters UE rejects in a long package name (`INVALID_LONGPACKAGENAME_CHARACTERS`
/// in `PackageName.h`). `.` is included because it separates the package from the
/// object path, which callers strip before testing.
const INVALID_PACKAGE_NAME_CHARACTERS: &str = "\\:*?\"<>|' ,.&!~\n\r\t@#(){}[]=;^%$`";

/// Whether a candidate path is shaped like a mount-rooted package path
/// (`/MountRoot/Name`).
///
/// String properties hold plenty of other slash-prefixed text — an HLSL source
/// block whose comment lines begin with `//` reduces to something that passes a
/// naive segment count — so the segments have to satisfy UE's own naming rule. A
/// single segment is rejected too: every package lives under a mount root.
fn is_package_shaped(path: &str) -> bool {
    if path.contains(|character| INVALID_PACKAGE_NAME_CHARACTERS.contains(character)) {
        return false;
    }
    let mut segments = path.split('/').filter(|segment| !segment.is_empty());
    segments.next().is_some() && segments.next().is_some()
}

/// Cross-check the package paths named by decoded values against the linker
/// tables, so the residue those tables cannot hold becomes a bounded list.
///
/// `/Script/` paths are excluded: they name engine and plugin modules, not
/// assets. So is the package's own name, which UE writes into its own soft
/// references.
pub(crate) fn build_reference_evidence(
    package_name: &str,
    tables: &AssetReferences,
    exports: &[AssetExport],
    graphs: &[LogicGraph],
) -> ReferenceEvidence {
    let recorded = tables
        .assets
        .iter()
        .chain(&tables.scripts)
        .chain(&tables.soft)
        .map(|reference| reference.to_ascii_lowercase())
        .chain(std::iter::once(package_name.to_ascii_lowercase()))
        .collect::<BTreeSet<_>>();

    let mut property_values = BTreeSet::new();
    let mut bytecode = BTreeSet::new();
    for export in exports {
        for property in &export.properties {
            collect_package_paths_from_value(&property.value, &mut property_values);
        }
        // Disassembled script names its targets directly. An object or function
        // constant resolves through the import table and will simply confirm what
        // the tables already hold; a soft-object constant is a runtime path and is
        // exactly the kind the tables cannot record.
        let Some(code) = export
            .script
            .as_ref()
            .and_then(|script| script.bytecode.as_ref())
        else {
            continue;
        };
        for reference in &code.references {
            if reference.kind == BYTECODE_FUNCTION_NAME_KIND {
                continue;
            }
            if let Some(package) = package_path_from_object_path(&reference.target) {
                bytecode.insert(package);
            }
        }
    }

    let mut pin_default_values = BTreeSet::new();
    let mut pin_default_objects = BTreeSet::new();
    for graph in graphs {
        for node in &graph.nodes {
            for pin in &node.pins {
                for default in [&pin.default_value, &pin.autogenerated_default_value] {
                    if let Some(package) =
                        default.as_deref().and_then(package_path_from_object_path)
                    {
                        pin_default_values.insert(package);
                    }
                }
                for value in [&pin.default_object, &pin.default_text]
                    .into_iter()
                    .flatten()
                {
                    collect_package_paths_from_value(value, &mut pin_default_objects);
                }
            }
            for pin in &node.user_defined_pins {
                if let Some(package) = pin
                    .default_value
                    .as_deref()
                    .and_then(package_path_from_object_path)
                {
                    pin_default_values.insert(package);
                }
            }
        }
    }

    let mut sources = ReferenceEvidenceSources::default();
    let mut all = BTreeSet::new();
    for (bucket, count) in [
        (property_values, &mut sources.property_values),
        (pin_default_values, &mut sources.pin_default_values),
        (pin_default_objects, &mut sources.pin_default_objects),
        (bytecode, &mut sources.bytecode),
    ] {
        for package in bucket {
            if package.starts_with(SCRIPT_PATH_PREFIX) || !is_package_shaped(&package) {
                continue;
            }
            *count += 1;
            all.insert(package);
        }
    }

    let mut value_only_packages = Vec::new();
    let mut confirmed_by_tables = 0;
    for package in &all {
        if recorded.contains(&package.to_ascii_lowercase()) {
            confirmed_by_tables += 1;
        } else {
            value_only_packages.push(package.clone());
        }
    }

    ReferenceEvidence {
        value_packages: all.len(),
        confirmed_by_tables,
        value_only_packages,
        sources,
    }
}
