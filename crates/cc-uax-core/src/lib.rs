mod analysis;
mod decode;
mod diagnostic;
mod model;
mod name;
mod object;
#[allow(dead_code)]
mod output;
mod package;
mod pin;
mod property;
mod reader;
#[allow(dead_code)]
mod references;
mod semantic_model;
mod summary;
mod version;

pub use analysis::PackageView;
pub use model::*;
pub use semantic_model::*;

#[cfg(test)]
pub(crate) use diagnostic::{ByteRangePreview, Diagnostic, Severity};
#[cfg(test)]
pub(crate) use output::OutputSections;
#[cfg(test)]
pub(crate) use package::Package;
#[cfg(test)]
pub(crate) use references::{
    MountMap, collect_package_references, package_path_from_relative,
    package_path_from_relative_with_mounts, referenced_packages_from_bytes,
};

#[cfg(test)]
pub(crate) fn parse_to_json(
    data: &[u8],
    sections: &output::OutputSections,
) -> anyhow::Result<serde_json::Value> {
    Ok(package::Package::parse(data)?.decode_to_json(data, sections))
}

#[cfg(test)]
mod tests;
