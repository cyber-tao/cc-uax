use crate::output::sections::OutputSections;
use crate::package::Package;
use serde_json::Value;

impl Package {
    pub(crate) fn decode_to_json(&self, data: &[u8], opts: &OutputSections) -> Value {
        self.decode(data, opts).to_json()
    }
}
