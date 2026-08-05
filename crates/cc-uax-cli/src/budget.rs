//! Output-size budgeting for rendered JSON reports.
//!
//! An AI tool that invokes `cc-uax` has a bounded context window. A single large
//! Blueprint can render into megabytes of JSON (StackOBot's `CR_Bot_Correction`
//! is ~4.7 MB), which can overflow that window. `--max-output-bytes` lets the
//! caller cap the rendered size to whatever space it has left.
//!
//! Budgeting is a **presentation concern only**. It never alters evidence: the
//! skeleton — `schema_version`, `status`, `summary`, `coverage`, `capabilities`,
//! `diagnostics`, `known_opaque`, `stats`, `analysis`, `layout`, `mounts`,
//! `entry_points`, and the project `reachability` roots and counts — is
//! preserved. Heavy detail (property values, pins, graph elements, whole
//! sections, and large reachability package lists) is elided in a deterministic
//! priority order and an `output` block records exactly what was dropped so the
//! caller can re-query a narrower `--focus`/`--view`.

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{Value, json};

/// Headroom reserved for the injected `output` block so the final document still
/// fits the caller's byte budget after the block is added.
const OUTPUT_BLOCK_RESERVE: usize = 512;

/// A named elision tier: a label plus the transform it applies to the report.
type ElisionTier = (&'static str, fn(&mut Value) -> usize);

/// Render `value` as JSON constrained to at most `budget` UTF-8 bytes.
///
/// When the full render already fits, an `output` block with `truncated=false`
/// is still added (the caller opted in by passing a budget). When it does not
/// fit, heavy detail is elided in priority order until it fits or only the
/// skeleton remains; the skeleton is always emitted as valid JSON even if it
/// alone exceeds a very small budget.
pub(crate) fn render_within_budget<T: Serialize>(
    value: &T,
    budget: usize,
    compact: bool,
) -> Result<String> {
    let mut root =
        serde_json::to_value(value).context("failed to serialize report for budgeting")?;
    if !root.is_object() {
        // Our reports are always JSON objects; anything else is emitted as-is.
        return render(&root, compact);
    }

    let target = budget.saturating_sub(OUTPUT_BLOCK_RESERVE);
    let mut elided: Vec<Value> = Vec::new();
    let detail_steps: [ElisionTier; 3] = [
        ("property_values", tier_property_values),
        ("pins", tier_pins),
        ("graph_elements", tier_graph_elements),
    ];
    for (label, step) in detail_steps {
        if measure(&root, compact) <= target {
            break;
        }
        let dropped = step(&mut root);
        if dropped > 0 {
            elided.push(json!({ "section": label, "dropped_elements": dropped }));
        }
    }
    // Budget-aware: keep as many leading section elements as fit before the next
    // tier drops whole sections, so a tight budget still returns leading detail.
    if measure(&root, compact) > target {
        let dropped = tier_truncate_sections(&mut root, target, compact);
        if dropped > 0 {
            elided.push(json!({ "section": "section_truncation", "dropped_elements": dropped }));
        }
    }
    let tail_steps: [ElisionTier; 2] = [
        ("sections", tier_sections),
        ("reachability_sets", tier_reachability_sets),
    ];
    for (label, step) in tail_steps {
        if measure(&root, compact) <= target {
            break;
        }
        let dropped = step(&mut root);
        if dropped > 0 {
            elided.push(json!({ "section": label, "dropped_elements": dropped }));
        }
    }

    let truncated = !elided.is_empty();
    // Two-pass to report the emitted size: render once with a zero placeholder to
    // size the document, then embed that length. The embedded value is within a
    // few bytes of the true final length (digit count of the number itself).
    // `truncated` also covers the case where the skeleton floor alone still
    // exceeds a very small budget with nothing left to elide.
    let reduced = measure(&root, compact);
    let truncated = truncated || reduced > budget;
    insert_output_block(&mut root, truncated, budget, 0, &elided);
    let emitted = measure(&root, compact);
    insert_output_block(&mut root, truncated, budget, emitted, &elided);
    render(&root, compact)
}

fn insert_output_block(
    root: &mut Value,
    truncated: bool,
    budget: usize,
    emitted: usize,
    elided: &[Value],
) {
    if let Value::Object(map) = root {
        map.insert(
            "output".to_string(),
            json!({
                "truncated": truncated,
                "budget_bytes": budget,
                "emitted_bytes": emitted,
                "elided": elided,
            }),
        );
    }
}

fn render(value: &Value, compact: bool) -> Result<String> {
    if compact {
        serde_json::to_string(value)
    } else {
        serde_json::to_string_pretty(value)
    }
    .context("failed to render JSON")
}

fn measure(value: &Value, compact: bool) -> usize {
    render(value, compact).map_or(usize::MAX, |text| text.len())
}

fn is_elided(value: &Value) -> bool {
    value
        .as_object()
        .is_some_and(|object| object.contains_key("@elided"))
}

/// Tier 1: replace every tagged-property `value` payload with an elision marker,
/// at any nesting depth (decoded struct values also carry nested `properties`).
fn tier_property_values(value: &mut Value) -> usize {
    let mut dropped = 0;
    if let Value::Object(map) = value
        && let Some(Value::Array(properties)) = map.get_mut("properties")
    {
        for property in properties.iter_mut() {
            if let Value::Object(entry) = property
                && let Some(payload) = entry.get_mut("value")
                && !is_elided(payload)
            {
                *payload = json!({ "@elided": "property value" });
                dropped += 1;
            }
        }
    }
    match value {
        Value::Array(items) => {
            for item in items.iter_mut() {
                dropped += tier_property_values(item);
            }
        }
        Value::Object(map) => {
            for child in map.values_mut() {
                dropped += tier_property_values(child);
            }
        }
        _ => {}
    }
    dropped
}

/// Tier 2: drop pin arrays (types, defaults, sub-pins), keeping their counts.
fn tier_pins(value: &mut Value) -> usize {
    elide_keyed_collections(
        value,
        &["pins", "user_defined_pins", "orphaned_pins", "sub_pins"],
        false,
    )
}

/// Tier 3: drop graph-internal element arrays (nodes/states/edges/links).
fn tier_graph_elements(value: &mut Value) -> usize {
    elide_keyed_collections(value, &["nodes", "states", "edges", "links"], false)
}

/// Top-level detail sections, in priority order, that the truncation and
/// wholesale-elision tiers operate on. The evidence skeleton
/// (status/coverage/capabilities/diagnostics/known_opaque/reachability) is not
/// listed here and is therefore always preserved.
const DETAIL_SECTION_KEYS: [&str; 10] = [
    "exports",
    "graphs",
    "rigvm_graphs",
    "pcg_graphs",
    "state_tree_graphs",
    "inventory",
    "focused",
    "forward",
    "reverse",
    "ownership_closure",
];

/// Tier 4 (skeleton): drop whole top-level detail sections, keeping the evidence
/// skeleton (status/coverage/capabilities/diagnostics/known_opaque/reachability).
fn tier_sections(value: &mut Value) -> usize {
    elide_keyed_collections(value, &DETAIL_SECTION_KEYS, true)
}

/// Number of elements in a section, whether it is a JSON array or object.
fn section_len(section: &Value) -> usize {
    match section {
        Value::Array(items) => items.len(),
        Value::Object(entries) => entries.len(),
        _ => 0,
    }
}

/// Truncate a section to its first `cap` elements, appending an `{"@elided": n}`
/// marker (a trailing array element, or an `@elided` key for an object) that
/// records the dropped count. Returns `None` when the section already fits `cap`.
fn truncate_section(section: &Value, cap: usize) -> Option<Value> {
    match section {
        Value::Array(items) if items.len() > cap => {
            let mut kept: Vec<Value> = items.iter().take(cap).cloned().collect();
            kept.push(json!({ "@elided": items.len() - cap }));
            Some(Value::Array(kept))
        }
        Value::Object(entries) if entries.len() > cap => {
            let mut kept = serde_json::Map::new();
            for (key, child) in entries.iter().take(cap) {
                kept.insert(key.clone(), child.clone());
            }
            kept.insert("@elided".to_string(), json!(entries.len() - cap));
            Some(Value::Object(kept))
        }
        _ => None,
    }
}

fn restore_sections(root: &mut Value, originals: &[(String, Value)]) {
    if let Value::Object(map) = root {
        for (key, original) in originals {
            map.insert(key.clone(), original.clone());
        }
    }
}

fn apply_section_cap(root: &mut Value, originals: &[(String, Value)], cap: usize) -> usize {
    let mut dropped = 0;
    if let Value::Object(map) = root {
        for (key, original) in originals {
            if let Some(truncated) = truncate_section(original, cap) {
                dropped += section_len(original) - cap;
                map.insert(key.clone(), truncated);
            }
        }
    }
    dropped
}

/// Budget-aware truncation tier: keep as many leading elements of the large
/// top-level sections as fit within `target`, appending `{"@elided": n}` markers.
/// A binary search finds the largest uniform per-section element cap whose render
/// fits, so a tight budget returns leading structural detail instead of nothing.
/// Sections that cannot fit even at cap 0 are left for the wholesale `sections`
/// tier. Returns the number of dropped elements.
fn tier_truncate_sections(root: &mut Value, target: usize, compact: bool) -> usize {
    let Value::Object(map) = root else {
        return 0;
    };
    let mut originals: Vec<(String, Value)> = Vec::new();
    for key in DETAIL_SECTION_KEYS {
        if let Some(child) = map.get(key)
            && section_len(child) > 1
            && !is_elided(child)
        {
            originals.push((key.to_string(), child.clone()));
        }
    }
    if originals.is_empty() {
        return 0;
    }
    let max_len = originals
        .iter()
        .map(|(_, section)| section_len(section))
        .max()
        .unwrap_or(0);

    // Largest cap in [0, max_len] whose render fits `target`.
    let mut lo = 0usize;
    let mut hi = max_len;
    let mut best: Option<usize> = None;
    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        restore_sections(root, &originals);
        apply_section_cap(root, &originals, mid);
        if measure(root, compact) <= target {
            best = Some(mid);
            lo = mid + 1;
        } else if mid == 0 {
            break;
        } else {
            hi = mid - 1;
        }
    }

    restore_sections(root, &originals);
    match best {
        Some(cap) => apply_section_cap(root, &originals, cap),
        None => 0,
    }
}

/// Tier 5 (deepest): drop the large project `reachability` package lists, keeping
/// `configured_roots` and the numeric `failed_assets` count. These are the last
/// to go because they summarize resource reachability.
fn tier_reachability_sets(value: &mut Value) -> usize {
    elide_keyed_collections(
        value,
        &[
            "reachable_runtime_packages",
            "ownership_closure_members",
            "unreachable_project_assets",
            "isolated_project_assets",
            "partial_packages",
            "unsupported_packages",
        ],
        false,
    )
}

/// Replace arrays/objects stored under any of `keys` with an `{"@elided": count}`
/// marker, returning the number of elements dropped. When `top_only` is set only
/// the root object's direct children are considered (the skeleton tier).
fn elide_keyed_collections(value: &mut Value, keys: &[&str], top_only: bool) -> usize {
    let mut dropped = 0;
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if keys.contains(&key.as_str()) {
                    let count = match child {
                        Value::Array(items) => items.len(),
                        Value::Object(entries) => entries.len(),
                        _ => 0,
                    };
                    if count > 0 && !is_elided(child) {
                        *child = json!({ "@elided": count });
                        dropped += count;
                    }
                } else if !top_only {
                    dropped += elide_keyed_collections(child, keys, top_only);
                }
            }
        }
        Value::Array(items) if !top_only => {
            for item in items.iter_mut() {
                dropped += elide_keyed_collections(item, keys, top_only);
            }
        }
        _ => {}
    }
    dropped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_report() -> Value {
        json!({
            "schema_version": 1,
            "status": "partial",
            "summary": { "package_name": "BigBlueprint" },
            "coverage": { "exports_total": 2, "properties_decoded": 3 },
            "capabilities": [ { "kind": "tagged_properties", "status": "partial" } ],
            "diagnostics": [],
            "known_opaque": [ { "path": "/x", "kind": "capability" } ],
            "exports": [
                {
                    "index": 1,
                    "name": "Node",
                    "properties": [
                        { "name": "Big", "type_name": "StructProperty", "value": { "blob": "x".repeat(4000) } }
                    ],
                    "pins": [ { "name": "Exec", "default": "y".repeat(2000) } ]
                }
            ],
            "graphs": [ { "name": "EventGraph", "nodes": [ {"id": 1}, {"id": 2} ], "edges": [ {"a": 1} ] } ]
        })
    }

    #[test]
    fn small_budget_stays_valid_json_and_preserves_skeleton() {
        let report = sample_report();
        let text = render_within_budget(&report, 512, true).unwrap();
        assert!(text.len() <= 512 || text.contains("\"truncated\":true"));
        let parsed: Value = serde_json::from_str(&text).unwrap();
        // Skeleton evidence is never elided.
        assert_eq!(parsed["schema_version"], 1);
        assert_eq!(parsed["status"], "partial");
        assert_eq!(parsed["coverage"]["exports_total"], 2);
        assert_eq!(parsed["capabilities"][0]["kind"], "tagged_properties");
        assert_eq!(parsed["known_opaque"][0]["kind"], "capability");
        // Truncation is recorded and heavy detail is gone.
        assert_eq!(parsed["output"]["truncated"], true);
        assert!(
            parsed["output"]["elided"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["section"] == "property_values")
        );
    }

    #[test]
    fn below_skeleton_budget_still_preserves_all_evidence() {
        // A budget far smaller than the skeleton must still emit valid JSON that
        // keeps every evidence field; only heavy detail is shed.
        let report = sample_report();
        let text = render_within_budget(&report, 10, true).unwrap();
        let parsed: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["output"]["truncated"], true);
        assert_eq!(parsed["schema_version"], 1);
        assert_eq!(parsed["status"], "partial");
        assert_eq!(parsed["coverage"]["exports_total"], 2);
        assert_eq!(parsed["capabilities"][0]["kind"], "tagged_properties");
        assert_eq!(parsed["known_opaque"][0]["kind"], "capability");
    }

    #[test]
    fn generous_budget_keeps_all_detail_but_marks_not_truncated() {
        let report = sample_report();
        let text = render_within_budget(&report, 100_000, false).unwrap();
        let parsed: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["output"]["truncated"], false);
        assert_eq!(parsed["output"]["elided"].as_array().unwrap().len(), 0);
        // Full detail is retained.
        assert!(parsed["exports"][0]["properties"][0]["value"]["blob"].is_string());
        assert_eq!(parsed["graphs"][0]["nodes"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn property_values_are_elided_before_graph_nodes() {
        // A budget that only requires shedding property values must keep graph
        // nodes intact (deterministic priority order).
        let report = sample_report();
        let full = serde_json::to_string(&report).unwrap().len();
        // Target just under the full size so the first tier alone suffices.
        let text = render_within_budget(&report, full - 100, true).unwrap();
        let parsed: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            parsed["exports"][0]["properties"][0]["value"]["@elided"],
            "property value"
        );
        // Graph nodes survive because earlier tiers freed enough room.
        assert!(parsed["graphs"][0]["nodes"].is_array());
        let sections: Vec<&str> = parsed["output"]["elided"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["section"].as_str().unwrap())
            .collect();
        assert_eq!(sections, vec!["property_values"]);
    }

    #[test]
    fn keeps_map_type_sections_like_forward_reverse() {
        let report = json!({
            "schema_version": 2,
            "status": "complete",
            "stats": { "indexed": 3 },
            "forward": { "a": ["b"], "b": ["c"], "c": [] },
            "reverse": { "b": ["a"], "c": ["b"] },
            "inventory": [ {"package": "a"}, {"package": "b"}, {"package": "c"} ]
        });
        let text = render_within_budget(&report, 200, true).unwrap();
        let parsed: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["stats"]["indexed"], 3);
        // Skeleton preserved; large sections elided with counts.
        assert!(parsed["output"]["truncated"].as_bool().unwrap());
    }

    #[test]
    fn moderate_budget_truncates_sections_keeping_leading_elements() {
        let exports: Vec<Value> = (0..500)
            .map(|i| json!({ "index": i, "name": format!("Export_{i}"), "class": "SomeClass" }))
            .collect();
        let report = json!({
            "schema_version": 3,
            "status": "complete",
            "summary": { "package_name": "Big" },
            "coverage": { "exports_total": 500 },
            "exports": exports,
        });
        let budget = 3000;
        let text = render_within_budget(&report, budget, true).unwrap();
        assert!(
            text.len() <= budget,
            "emitted {} exceeds budget {budget}",
            text.len()
        );
        let parsed: Value = serde_json::from_str(&text).unwrap();
        // Skeleton is preserved.
        assert_eq!(parsed["status"], "complete");
        assert_eq!(parsed["coverage"]["exports_total"], 500);
        // Exports remain an array of leading elements plus a trailing marker,
        // rather than being dropped wholesale.
        let kept = parsed["exports"].as_array().unwrap();
        assert!(
            kept.len() >= 3,
            "expected leading exports retained, got {}",
            kept.len()
        );
        assert_eq!(kept[0]["index"], 0);
        assert_eq!(kept[1]["index"], 1);
        assert!(
            kept.last().unwrap()["@elided"].is_number(),
            "expected a trailing @elided marker"
        );
        assert_eq!(parsed["output"]["truncated"], true);
        assert!(
            parsed["output"]["elided"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry["section"] == "section_truncation")
        );
        // The budget is used well rather than collapsing to the skeleton floor.
        assert!(
            text.len() >= budget / 2,
            "truncation under-filled the budget: {} of {budget}",
            text.len()
        );
    }
}
