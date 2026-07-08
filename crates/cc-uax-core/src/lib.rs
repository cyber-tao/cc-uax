mod decode;
mod diagnostic;
mod name;
mod object;
mod output;
mod package;
mod pin;
mod property;
mod reader;
mod references;
mod summary;
mod version;

pub use decode::DecodeReport;
pub use diagnostic::{ByteRangePreview, Diagnostic, Severity};
pub use output::OutputSections;
pub use package::Package;
pub use references::{
    MountMap, collect_package_references, package_path_from_relative,
    package_path_from_relative_with_mounts, referenced_packages_from_bytes,
};

#[cfg(test)]
mod tests;
