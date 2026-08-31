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

/// Platform variants sharing one Content tree must stay scannable.
///
/// Which tree to scan is unambiguous; only which descriptor supplies entry points
/// is not. Discovery therefore records the ambiguity instead of failing, and
/// naming one `.uproject` still resolves it.
#[test]
fn ambiguous_project_files_leave_the_descriptor_unresolved_not_the_scan_failed() {
    let root = temp_project("ambiguous");
    std::fs::write(root.join("One.uproject"), b"{}").unwrap();
    std::fs::write(root.join("Two.uproject"), b"{}").unwrap();

    for layout in [
        ProjectLayout::discover(&root).unwrap(),
        ProjectLayout::discover(root.join("Content")).unwrap(),
    ] {
        assert!(
            layout.project_file().is_none(),
            "no single descriptor can be chosen"
        );
        assert_eq!(
            layout.ambiguous_project_files().len(),
            2,
            "both candidates must be retained so the scan can name them"
        );
        assert!(layout.content_root().ends_with("Content"));
    }

    let from_file = ProjectLayout::discover(root.join("One.uproject")).unwrap();
    assert_eq!(
        from_file.project_file().unwrap(),
        std::fs::canonicalize(root.join("One.uproject")).unwrap()
    );
    assert!(from_file.ambiguous_project_files().is_empty());

    std::fs::remove_dir_all(root).unwrap();
}
