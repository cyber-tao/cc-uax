use crate::reader::{RawName, Reader};
use crate::version::ue4;
use anyhow::Result;

pub struct NameMap {
    pub names: Vec<String>,
}

impl NameMap {
    pub fn parse(reader: &mut Reader, offset: i32, count: i32, ue4_version: i32) -> Result<Self> {
        let mut names = Vec::with_capacity(count.max(0) as usize);
        if offset > 0 && count > 0 {
            reader.seek(offset as u64)?;
            let has_hashes = ue4_version >= ue4::NAME_HASHES_SERIALIZED;
            for _ in 0..count {
                let s = reader.read_fstring()?;
                if has_hashes {
                    reader.skip(4)?;
                }
                names.push(s);
            }
        }
        Ok(NameMap { names })
    }

    pub fn get(&self, index: i32) -> Option<&str> {
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
