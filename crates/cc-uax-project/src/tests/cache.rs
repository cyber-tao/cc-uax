use super::common::temp_project;
use crate::{CachePathPolicy, ProjectLayout};

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
