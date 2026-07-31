use super::common::temp_project;
use crate::cache::{CacheEntry, ProjectCache};
use crate::{CachePathPolicy, ProjectLayout};
use std::collections::HashMap;

fn cache_entry(mtime: i64, size: i64, references: &[&str]) -> CacheEntry {
    CacheEntry {
        mtime,
        size,
        parse_ok: true,
        references: references.iter().map(|value| value.to_string()).collect(),
        analysis: None,
        parse_error: None,
    }
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
    assert_eq!(system.file_name().unwrap(), "project-index-v2.sqlite");

    std::fs::remove_dir_all(root).unwrap();
}
