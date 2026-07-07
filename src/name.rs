use crate::reader::{RawName, Reader};
use crate::version::ue4;
use anyhow::{Result, bail};

pub struct NameMap {
    pub names: Vec<String>,
}

impl NameMap {
    pub fn parse(reader: &mut Reader, offset: i32, count: i32, ue4_version: i32) -> Result<Self> {
        if count < 0 {
            bail!("name count out of range: {count}");
        }
        if count == 0 {
            return Ok(NameMap { names: Vec::new() });
        }
        if offset <= 0 {
            bail!("name table offset must be positive when name count is {count}");
        }

        reader.seek(offset as u64)?;
        let has_hashes = ue4_version >= ue4::NAME_HASHES_SERIALIZED;
        let min_entry_bytes = if has_hashes { 8u64 } else { 4u64 };
        if (count as u64).saturating_mul(min_entry_bytes) > reader.remaining() {
            bail!("name table count out of range: {count}");
        }

        let mut names = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let s = reader.read_fstring()?;
            if has_hashes {
                reader.skip(4)?;
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
