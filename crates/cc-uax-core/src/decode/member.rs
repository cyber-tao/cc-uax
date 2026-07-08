use super::MemberRef;
use crate::property::PropertyEntry;
use serde_json::Value;

pub(super) fn distill_member(props: &[PropertyEntry]) -> Option<MemberRef> {
    const REF_PROPS: [&str; 4] = [
        "FunctionReference",
        "EventReference",
        "VariableReference",
        "DelegateReference",
    ];
    for e in props {
        if !REF_PROPS.contains(&e.name.as_str()) {
            continue;
        }
        let inner = match e.value.get("properties").and_then(Value::as_array) {
            Some(a) => a,
            None => continue,
        };
        let mut name = None;
        let mut parent = None;
        for p in inner {
            match p.get("name").and_then(Value::as_str) {
                Some("MemberName") => {
                    name = p.get("value").and_then(Value::as_str).map(str::to_owned);
                }
                Some("MemberParent") => {
                    parent = p.get("value").cloned();
                }
                _ => {}
            }
        }
        if let Some(name) = name {
            return Some(MemberRef { name, parent });
        }
    }
    None
}
