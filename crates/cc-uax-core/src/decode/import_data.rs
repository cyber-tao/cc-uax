//! `UAssetImportData::Serialize` writes an `FString` of JSON *before*
//! `Super::Serialize` (`AssetImportData.cpp`, UE5.0–5.8): the source files the
//! asset was imported from, as `FAssetImportInfo::ToJson` prints them. Every
//! imported mesh, animation, texture and Datasmith/Interchange asset carries one
//! such export, so leaving the string undecoded made the tagged block of every
//! one of them start in the middle of a JSON array — on a UE5.0–5.3 project the
//! single largest source of `partial` assets — and threw away the one piece of
//! provenance evidence the class exists to hold.

use super::window::ExportSerialWindow;
use crate::reader::Reader;
use crate::version::ue4;

/// One entry of `FAssetImportInfo::SourceFiles`, as `ToJson` prints it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportSourceFile {
    pub(crate) relative_filename: String,
    /// Unix timestamp of the source file when it was imported, when present.
    pub(crate) timestamp: Option<i64>,
    pub(crate) file_hash: Option<String>,
    pub(crate) display_label: Option<String>,
}

/// A class whose `Serialize` opens with the `UAssetImportData` JSON prefix.
///
/// `UAssetImportData` and every subclass (`UFbxAssetImportData` and its
/// per-type children, `UInterchangeAssetImportData`, the Datasmith
/// `*ImportData` classes) inherit the override, and Unreal names all of them
/// with the `ImportData` suffix. The payload itself is the real check: the
/// prefix must be a well-formed `FString` holding a JSON array, so a class that
/// merely shares the suffix without the prefix decodes nothing and loses nothing.
pub(crate) fn is_asset_import_data_class(class_full: &str) -> bool {
    class_full.starts_with("/Script/") && class_full.ends_with("ImportData")
}

/// Whether the package writes the JSON prefix at all: from
/// `VER_UE4_ASSET_IMPORT_DATA_AS_JSON`, and only when the archive is not
/// `FilterEditorOnly` (the string is editor provenance).
pub(crate) fn writes_import_data_prefix(file_version_ue4: i32, filter_editor_only: bool) -> bool {
    file_version_ue4 >= ue4::ASSET_IMPORT_DATA_AS_JSON && !filter_editor_only
}

/// Reads the JSON prefix at the start of the export window and returns the
/// source files together with the offset where the tagged block begins.
///
/// `None` means the bytes are not that prefix and the caller must leave the
/// window untouched: the string has to be a complete `FString`, its content a
/// JSON array (`[` … `]`), and — when UE declared the tagged-property range — it
/// has to end exactly where that range starts. Anything else is not evidence.
pub(crate) fn decode_import_data_prefix(
    reader: &mut Reader,
    window: ExportSerialWindow,
) -> Option<(Vec<ImportSourceFile>, u64)> {
    if reader.seek(window.serial_start).is_err() {
        return None;
    }
    // A declared range bounds the string; otherwise the export end does.
    let limit = if window.has_declared_property_range {
        window.property_start
    } else {
        window.serial_end
    };
    let json = reader
        .read_fstring_within(limit, "asset import data json")
        .ok()?;
    let prefix_end = reader.pos();
    if window.has_declared_property_range && prefix_end != window.property_start {
        return None;
    }
    let source_files = parse_source_files(&json)?;
    Some((source_files, prefix_end))
}

/// Parses the fixed shape `FAssetImportInfo::ToJson` prints: an array of flat
/// objects whose values are all strings. This is not a general JSON parser and
/// does not need to be; UE itself prints the string with `Printf` and does not
/// escape the values, so a filename containing `"` would break UE's own
/// `FromJson` as well. Returns `None` when the text is not that shape.
pub(crate) fn parse_source_files(json: &str) -> Option<Vec<ImportSourceFile>> {
    let body = json.trim();
    let body = body.strip_prefix('[')?.strip_suffix(']')?;
    let mut files = Vec::new();
    let mut rest = body.trim_start();
    while !rest.is_empty() {
        let object_start = rest.find('{')?;
        let object_end = rest[object_start..].find('}')? + object_start;
        let object = &rest[object_start + 1..object_end];
        files.push(ImportSourceFile {
            relative_filename: string_field(object, "RelativeFilename")?,
            timestamp: string_field(object, "Timestamp").and_then(|value| value.parse().ok()),
            file_hash: string_field(object, "FileMD5").filter(|value| !value.is_empty()),
            display_label: string_field(object, "DisplayLabelName")
                .filter(|value| !value.is_empty()),
        });
        rest = rest[object_end + 1..].trim_start();
        rest = rest.strip_prefix(',').unwrap_or(rest).trim_start();
    }
    Some(files)
}

/// `"Key" : "value"` inside one flat JSON object, whitespace-tolerant.
fn string_field(object: &str, key: &str) -> Option<String> {
    let marker = format!("\"{key}\"");
    let key_end = object.find(&marker)? + marker.len();
    let after_key = object[key_end..].trim_start();
    let after_colon = after_key.strip_prefix(':')?.trim_start();
    let value = after_colon.strip_prefix('"')?;
    let value_end = value.find('"')?;
    Some(value[..value_end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_printf_shape_ue_writes() {
        let json = r#"[{ "RelativeFilename" : "../../../Src/Bot.fbx", "Timestamp" : "1700000000", "FileMD5" : "0123456789abcdef0123456789abcdef", "DisplayLabelName" : "" },{ "RelativeFilename" : "B.png", "Timestamp" : "-1", "FileMD5" : "", "DisplayLabelName" : "Base Color" }]"#;
        let files = parse_source_files(json).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].relative_filename, "../../../Src/Bot.fbx");
        assert_eq!(files[0].timestamp, Some(1_700_000_000));
        assert_eq!(
            files[0].file_hash.as_deref(),
            Some("0123456789abcdef0123456789abcdef")
        );
        assert_eq!(files[0].display_label, None);
        assert_eq!(files[1].timestamp, Some(-1));
        assert_eq!(files[1].file_hash, None);
        assert_eq!(files[1].display_label.as_deref(), Some("Base Color"));
    }

    #[test]
    fn an_empty_array_is_zero_source_files() {
        assert_eq!(parse_source_files("[]"), Some(Vec::new()));
    }

    #[test]
    fn anything_that_is_not_the_array_is_rejected() {
        assert_eq!(parse_source_files(""), None);
        assert_eq!(parse_source_files("{}"), None);
        assert_eq!(parse_source_files("[{ \"Other\" : \"x\" }]"), None);
        assert_eq!(parse_source_files("[{ \"RelativeFilename\" : \"x\" "), None);
    }
}
