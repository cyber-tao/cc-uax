use super::common::*;
use crate::reader::Reader;
use crate::summary::PackageFileSummary;
use crate::{AssetView, PackageView, ReadExtent, package_read_extent};

/// Byte offset of `total_header_size` in a UE5 package below 1016, where it follows
/// the tag, the four version fields, the licensee version and the custom version count.
const TOTAL_HEADER_SIZE_POS: usize = 4 + 4 + 4 + 4 + 4 + 4 + 4;
const TRAILING_BULK_BYTES: usize = 1024;

/// A 1009 package with one tagged export, `TotalHeaderSize` patched to where the
/// export data starts, and a "bulk" tail nothing reads. Returns the bytes, the
/// header size, and the end of the last export.
fn package_with_bulk_tail() -> (Vec<u8>, u64, u64) {
    let mut builder = PackageBuilder::new(1009, (5, 3));
    let class = builder.script_class("/Script/Engine", "Thing");
    let object = builder.name("Obj") as i32;
    let value = builder.name("Value") as i32;
    let int = builder.name("IntProperty") as i32;
    let none = builder.name("None") as i32;
    let mut payload = Vec::new();
    push_legacy_tag_header(&mut payload, value, int, 4);
    push_legacy_tag_tail(&mut payload, 1009);
    push_i32(&mut payload, 42);
    push_raw_name(&mut payload, none);
    builder.exports.push(ExportSpec {
        class_index: class,
        outer_index: 0,
        object_name: object as usize,
        payload: payload.clone(),
        script_range: None,
    });
    let mut data = builder.build();
    let header_end = (data.len() - payload.len()) as u64;
    put_i32(&mut data, TOTAL_HEADER_SIZE_POS, header_end as i32);
    let last_export_end = data.len() as u64;
    data.extend_from_slice(&[0xEE; TRAILING_BULK_BYTES]);
    (data, header_end, last_export_end)
}

#[test]
fn the_read_extent_ends_at_the_last_export() {
    let (data, _, last_export_end) = package_with_bulk_tail();

    let extent = package_read_extent(&data, data.len() as u64).unwrap();

    assert_eq!(extent, ReadExtent::Prefix(last_export_end));
    assert_eq!(
        data.len() as u64 - last_export_end,
        TRAILING_BULK_BYTES as u64
    );
}

#[test]
fn analysing_the_prefix_reports_the_same_as_analysing_the_whole_file() {
    let (data, _, last_export_end) = package_with_bulk_tail();
    let file_len = data.len() as u64;

    let whole = PackageView::parse(&data).unwrap().analyze(AssetView::Full);
    let prefix = PackageView::parse_prefix(&data[..last_export_end as usize], file_len)
        .unwrap()
        .analyze(AssetView::Full);

    assert_eq!(prefix.coverage.bytes_total, file_len);
    assert_eq!(
        serde_json_crate::to_string(&prefix).unwrap(),
        serde_json_crate::to_string(&whole).unwrap()
    );
    assert_eq!(prefix.exports[0].properties[0].value, 42);
}

#[test]
fn a_head_shorter_than_the_header_asks_for_the_header() {
    let (data, header_end, _) = package_with_bulk_tail();
    let summary_end = PackageFileSummary::parse(&mut Reader::new(&data))
        .unwrap()
        .name_offset as usize;

    let extent = package_read_extent(&data[..summary_end], data.len() as u64).unwrap();

    assert_eq!(extent, ReadExtent::NeedHeader(header_end));
}

#[test]
fn an_extent_that_cannot_be_trusted_is_an_error_so_the_caller_reads_everything() {
    // No usable TotalHeaderSize.
    let (mut data, _, _) = package_with_bulk_tail();
    put_i32(&mut data, TOTAL_HEADER_SIZE_POS, 0);
    assert!(package_read_extent(&data, data.len() as u64).is_err());

    // A table that starts past the declared header end.
    let (mut data, _, _) = package_with_bulk_tail();
    put_i32(&mut data, TOTAL_HEADER_SIZE_POS, 100);
    assert!(package_read_extent(&data, data.len() as u64).is_err());

    // Out-of-scope stays out of scope.
    let error = package_read_extent(&build_ue4_stub(), 64).unwrap_err();
    assert!(error.is_out_of_scope());
}

/// The first bytes of a UE4 package: enough for the summary to reject it.
fn build_ue4_stub() -> Vec<u8> {
    let mut d = Vec::new();
    push_u32(&mut d, 0x9E2A_83C1);
    push_i32(&mut d, -7);
    push_i32(&mut d, 0);
    push_i32(&mut d, 522);
    push_i32(&mut d, 0);
    d
}
