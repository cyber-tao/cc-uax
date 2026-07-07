use crate::diagnostic::ByteRangePreview;
use serde_json::{Value, json};

pub(crate) fn tail_to_json(tail: &ByteRangePreview) -> Value {
    json!({
        "size": tail.size,
        "start": tail.start,
        "end": tail.end,
        "preview": tail.preview,
    })
}

pub(crate) fn name_or_null(s: String) -> Value {
    if s.is_empty() { Value::Null } else { json!(s) }
}
