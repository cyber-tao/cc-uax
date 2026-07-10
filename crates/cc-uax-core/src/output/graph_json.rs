use crate::analysis::build_logic_graphs;
use crate::decode::DecodeReport;
use serde_json::Value;

pub(crate) fn graphs_to_json(report: &DecodeReport<'_>) -> Value {
    serde_json::to_value(build_logic_graphs(report))
        .expect("typed logic graph serialization must succeed")
}
