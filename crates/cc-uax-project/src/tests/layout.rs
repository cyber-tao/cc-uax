use super::common::temp_project;
use crate::ProjectLayout;

#[test]
fn discovers_project_root_content_and_project_file() {
    let root = temp_project("layout");
    std::fs::write(root.join("Sample.uproject"), b"{}").unwrap();
    let nested = root.join("Content/Maps/Nested");
    std::fs::create_dir_all(&nested).unwrap();

    let from_root = ProjectLayout::discover(&root).unwrap();
    let from_content = ProjectLayout::discover(root.join("Content")).unwrap();
    let from_nested = ProjectLayout::discover(&nested).unwrap();
    let from_file = ProjectLayout::discover(root.join("Sample.uproject")).unwrap();

    assert_eq!(from_root, from_content);
    assert_eq!(from_root, from_nested);
    assert_eq!(from_root, from_file);
    assert_eq!(
        from_root.project_file().unwrap(),
        std::fs::canonicalize(root.join("Sample.uproject")).unwrap()
    );

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn rejects_ambiguous_project_files() {
    let root = temp_project("ambiguous");
    std::fs::write(root.join("One.uproject"), b"{}").unwrap();
    std::fs::write(root.join("Two.uproject"), b"{}").unwrap();

    let error = ProjectLayout::discover(&root).unwrap_err();
    assert!(error.to_string().contains("multiple .uproject"));

    std::fs::remove_dir_all(root).unwrap();
}
