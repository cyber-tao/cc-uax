use super::common::{package_with_soft_refs, temp_project};
use crate::{ReadScope, read_package_file};

// A header larger than the first read is fetched in a second, and the bulk tail
// after it is never touched.
#[test]
fn a_header_larger_than_the_first_read_is_read_in_full_and_the_tail_is_not() {
    let root = temp_project("package_file");
    let targets: Vec<String> = (0..3000)
        .map(|index| format!("/Game/Generated/Folder/Asset_{index:06}"))
        .collect();
    let targets: Vec<&str> = targets.iter().map(String::as_str).collect();
    let mut data = package_with_soft_refs(&targets);
    let header_len = data.len();
    assert!(
        header_len > 64 * 1024,
        "the fixture must outgrow the first read"
    );
    data.extend_from_slice(&vec![0xEE; 256 * 1024]);
    let path = root.join("Content/Big.uasset");
    std::fs::write(&path, &data).unwrap();

    let read = read_package_file(&path, ReadScope::Analysis).unwrap();

    assert_eq!(read.file_len, data.len() as u64);
    assert_eq!(read.bytes.len(), header_len);
    assert_eq!(read.bytes, data[..header_len]);

    std::fs::remove_dir_all(root).unwrap();
}

// Nothing can be trusted about a file the summary cannot describe, so it is read
// whole and the normal parse reports what it always reported.
#[test]
fn a_file_the_extent_cannot_describe_is_read_whole() {
    let root = temp_project("package_file_whole");
    let data = vec![0x42; 200 * 1024];
    let path = root.join("Content/NotAPackage.uasset");
    std::fs::write(&path, &data).unwrap();

    let read = read_package_file(&path, ReadScope::Analysis).unwrap();

    assert_eq!(read.bytes.len(), data.len());

    std::fs::remove_dir_all(root).unwrap();
}
