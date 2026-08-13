use super::common::temp_project;
use crate::cache::{CacheEntry, ProjectCache, UNKNOWN_MTIME};
use crate::{CachePathPolicy, ProjectLayout};
use std::collections::HashMap;

fn cache_entry(mtime: i64, size: i64, references: &[&str]) -> CacheEntry {
    CacheEntry {
        mtime,
        size,
        parse_ok: true,
        references: references.iter().map(|value| value.to_string()).collect(),
        owned_sublevels: Vec::new(),
        analysis: None,
        parse_error: None,
    }
}

#[test]
fn unknown_mtime_is_never_fresh() {
    // A file whose mtime cannot be read must never satisfy the freshness check,
    // so it is re-parsed rather than reused from a possibly stale cache row.
    assert!(!cache_entry(UNKNOWN_MTIME, 10, &[]).is_fresh(UNKNOWN_MTIME, 10));
    let normal = cache_entry(5, 10, &[]);
    assert!(!normal.is_fresh(UNKNOWN_MTIME, 10));
    assert!(normal.is_fresh(5, 10));
}

#[test]
fn incremental_store_upserts_changes_and_deletes_removed_entries() {
    let root = temp_project("cache_store");
    let path = root.join("cache/index.sqlite");

    {
        let mut cache = ProjectCache::open(&path).unwrap();
        let mut current = HashMap::new();
        current.insert("A".to_string(), cache_entry(1, 10, &["/Game/X"]));
        current.insert("B".to_string(), cache_entry(2, 20, &[]));
        assert!(cache.store(&current).unwrap());
        assert!(!cache.store(&current).unwrap());
    }

    {
        let mut cache = ProjectCache::open(&path).unwrap();
        assert!(cache.lookup("A", 1, 10).is_some());
        assert!(cache.lookup("B", 2, 20).is_some());
        // Change A's stamp, drop B, add C.
        let mut current = HashMap::new();
        current.insert("A".to_string(), cache_entry(3, 11, &["/Game/Y"]));
        current.insert("C".to_string(), cache_entry(4, 40, &[]));
        assert!(cache.store(&current).unwrap());
    }

    let cache = ProjectCache::open(&path).unwrap();
    assert!(cache.lookup("A", 1, 10).is_none());
    assert!(cache.lookup("A", 3, 11).is_some());
    assert!(cache.lookup("B", 2, 20).is_none());
    assert!(cache.lookup("C", 4, 40).is_some());
    drop(cache);

    std::fs::remove_dir_all(root).unwrap();
}

// A tool upgrade can change decoded analysis for an unchanged file (same
// mtime/size), so a cache written by a different tool version must be dropped.
#[test]
fn cache_is_invalidated_when_the_tool_version_changes() {
    let root = temp_project("cache_toolver");
    let path = root.join("cache/index.sqlite");

    {
        let mut cache = ProjectCache::open(&path).unwrap();
        let mut current = HashMap::new();
        current.insert("A".to_string(), cache_entry(1, 10, &["/Game/X"]));
        assert!(cache.store(&current).unwrap());
    }

    // Simulate a cache produced by an older binary with different decoders.
    {
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute(
                "INSERT OR REPLACE INTO cache_meta (key, value) VALUES ('tool_version', '0.0.0-old')",
                [],
            )
            .unwrap();
    }

    // Re-opening with the current tool version drops the stale analysis rows.
    let cache = ProjectCache::open(&path).unwrap();
    assert!(cache.lookup("A", 1, 10).is_none());
    assert!(cache.reset_reason().is_some());
    drop(cache);

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn disabled_and_custom_cache_policies_are_deterministic() {
    let root = temp_project("cache");
    let layout = ProjectLayout::discover(&root).unwrap();
    assert_eq!(CachePathPolicy::default(), CachePathPolicy::System);
    assert_eq!(CachePathPolicy::Disabled.resolve(&layout).unwrap(), None);

    let custom = root.join("cache/custom.sqlite");
    let first = CachePathPolicy::CustomFile(custom.clone())
        .resolve(&layout)
        .unwrap()
        .unwrap();
    let second = CachePathPolicy::CustomFile(custom.clone())
        .resolve(&layout)
        .unwrap()
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(first, custom);

    let system = CachePathPolicy::System.resolve(&layout).unwrap().unwrap();
    assert!(!system.starts_with(layout.project_root()));
    assert_eq!(system.file_name().unwrap(), "project-index.sqlite");

    std::fs::remove_dir_all(root).unwrap();
}
