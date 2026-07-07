//! JSON output layer: `Package::to_json` and the per-section serializers, plus the
//! export serial-window and Blueprint-pin serialization helpers.

mod json;
pub mod sections;

pub use sections::OutputSections;
