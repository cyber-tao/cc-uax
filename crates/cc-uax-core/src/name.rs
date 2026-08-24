use crate::reader::{FSTRING_LENGTH_BYTES, RawName, Reader, seek_to_table};
use crate::version::ue4;
use anyhow::Result;

/// `FNameEntrySerialized` writes `uint16 DummyHashes[2]` after the string
/// (`UnrealNames.cpp`).
const NAME_HASH_BYTES: u64 = 4;

pub struct NameMap {
    pub names: Vec<String>,
}

impl NameMap {
    pub fn parse(reader: &mut Reader, offset: i32, count: i32, ue4_version: i32) -> Result<Self> {
        let has_hashes = ue4_version >= ue4::NAME_HASHES_SERIALIZED;
        let min_entry_bytes = FSTRING_LENGTH_BYTES + if has_hashes { NAME_HASH_BYTES } else { 0 };
        if !seek_to_table(reader, "name table", offset, count, min_entry_bytes)? {
            return Ok(NameMap { names: Vec::new() });
        }

        let mut names = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let s = reader.read_fstring()?;
            if has_hashes {
                reader.skip(NAME_HASH_BYTES)?;
            }
            names.push(s);
        }
        Ok(NameMap { names })
    }

    fn get(&self, index: i32) -> Option<&str> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.names.get(i))
            .map(|s| s.as_str())
    }

    pub fn resolve(&self, index: i32, number: i32) -> String {
        let base = self
            .get(index)
            .map(|s| s.to_owned())
            .unwrap_or_else(|| format!("<invalid_name#{index}>"));
        if number == 0 {
            base
        } else {
            format!("{base}_{}", number as i64 - 1)
        }
    }

    pub fn resolve_raw(&self, raw: RawName) -> String {
        self.resolve(raw.index, raw.number)
    }
}
