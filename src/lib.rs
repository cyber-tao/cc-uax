pub mod decode;
pub mod diagnostic;
pub mod name;
pub mod object;
pub mod output;
pub mod package;
pub mod pin;
pub mod property;
pub mod reader;
pub mod references;
pub mod summary;
pub mod version;

pub use decode::{DecodeOptions, DecodeReport};
pub use diagnostic::{ByteRangePreview, Diagnostic, Severity};
pub use output::OutputSections;
pub use package::Package;
pub use references::{
    MountMap, collect_package_references, package_path_from_relative,
    package_path_from_relative_with_mounts, referenced_packages_from_bytes,
};
pub type SectionSet = OutputSections;
