//! Package-reference extraction from parsed import tables.
//!
//! Filesystem mounts, project scanning, reverse adjacency, and cache policy live
//! in `cc-uax-project`; the parser core only classifies references already
//! present in one package.

use crate::package::Package;
use std::collections::BTreeSet;

const PACKAGE_CLASS_NAME: &str = "Package";
const SCRIPT_PATH_PREFIX: &str = "/Script/";

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
