//! JSON output layer: `Package::to_json` and per-section serializers. Parsing
//! and per-export decoding live in `crate::decode`.

mod export_json;
mod package_json;
mod pin_json;
mod property_json;
mod report_json;
pub mod sections;

pub use sections::OutputSections;
