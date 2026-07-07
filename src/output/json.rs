use crate::decode::DecodeOptions;
use crate::output::sections::OutputSections;
use crate::package::Package;
use serde_json::Value;

impl Package {
    pub fn to_json(&self, data: &[u8], opts: &OutputSections) -> Value {
        self.decode(data, &DecodeOptions::from(opts)).to_json()
    }
}
