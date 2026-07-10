use crate::diagnostic::ByteRangePreview;
use crate::object::ObjectExport;
use crate::property::to_hex;
use crate::reader::Reader;

#[derive(Debug, Clone, Copy)]
pub(super) struct ExportSerialWindow {
    pub property_start: u64,
    pub property_end: u64,
    pub serial_end: u64,
}

pub(super) fn export_serial_window(
    exp: &ObjectExport,
    has_script: bool,
    file_len: u64,
) -> std::result::Result<Option<ExportSerialWindow>, String> {
    if exp.serial_size < 0 {
        return Err(format!("negative serial size {}", exp.serial_size));
    }
    if exp.serial_size == 0 {
        return Ok(None);
    }
    if exp.serial_offset < 0 {
        return Err(format!(
            "negative serial offset {} for non-empty export",
            exp.serial_offset
        ));
    }

    let serial_start = exp.serial_offset as u64;
    let serial_size = exp.serial_size as u64;
    let serial_end = serial_start
        .checked_add(serial_size)
        .ok_or_else(|| "serial range overflows u64".to_string())?;
    if serial_end > file_len {
        return Err(format!(
            "serial range [{serial_start}, {serial_end}) exceeds file length {file_len}"
        ));
    }

    if !has_script {
        return Ok(Some(ExportSerialWindow {
            property_start: serial_start,
            property_end: serial_end,
            serial_end,
        }));
    }

    let script_start = exp.script_serialization_start_offset;
    let script_end = exp.script_serialization_end_offset;
    if script_start == 0 && script_end == 0 {
        return Ok(Some(ExportSerialWindow {
            property_start: serial_start,
            property_end: serial_end,
            serial_end,
        }));
    }
    if script_start < 0 || script_end < script_start || script_end > exp.serial_size {
        return Err(format!(
            "script serialization range [{script_start}, {script_end}) is outside serial size {}",
            exp.serial_size
        ));
    }

    Ok(Some(ExportSerialWindow {
        property_start: serial_start
            .checked_add(script_start as u64)
            .ok_or_else(|| "script serialization start overflows u64".to_string())?,
        property_end: serial_start
            .checked_add(script_end as u64)
            .ok_or_else(|| "script serialization end overflows u64".to_string())?,
        serial_end,
    }))
}

pub(super) fn preview_range(reader: &mut Reader, start: u64, end: u64) -> ByteRangePreview {
    let size = end.saturating_sub(start);
    let preview_len = size.min(64) as usize;
    let _ = reader.seek(start);
    let preview = reader.read_bytes(preview_len).unwrap_or_default();
    let _ = reader.seek(end);
    ByteRangePreview {
        start,
        end,
        size,
        preview: to_hex(&preview),
    }
}
