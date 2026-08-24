use anyhow::{Result, bail};
use byteorder::{LittleEndian, ReadBytesExt};
use std::io::{Cursor, Read, Seek, SeekFrom};

/// Serialized size of an `FName` reference (name index + number). Counts and
/// windows are checked against this before a name is read.
pub const RAW_NAME_BYTES: u64 = 8;

/// The `int32` length that precedes every `FString` payload, and therefore the
/// smallest number of bytes an `FString` array entry can occupy.
pub const FSTRING_LENGTH_BYTES: u64 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Guid(pub [u32; 4]);

impl Guid {
    pub fn is_zero(&self) -> bool {
        self.0 == [0, 0, 0, 0]
    }

    pub fn to_hex(self) -> String {
        format!(
            "{:08X}{:08X}{:08X}{:08X}",
            self.0[0], self.0[1], self.0[2], self.0[3]
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawName {
    pub index: i32,
    pub number: i32,
}

pub struct Reader<'a> {
    cur: Cursor<&'a [u8]>,
    len: u64,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader {
            len: data.len() as u64,
            cur: Cursor::new(data),
        }
    }

    #[inline]
    pub fn pos(&self) -> u64 {
        self.cur.position()
    }

    #[inline]
    pub fn len(&self) -> u64 {
        self.len
    }

    #[inline]
    pub fn remaining(&self) -> u64 {
        self.len.saturating_sub(self.pos())
    }

    pub fn seek(&mut self, pos: u64) -> Result<()> {
        if pos > self.len {
            bail!(
                "seek out of range: target {} exceeds length {}",
                pos,
                self.len
            );
        }
        self.cur.seek(SeekFrom::Start(pos))?;
        Ok(())
    }

    pub fn skip(&mut self, n: u64) -> Result<()> {
        let target = self.pos().saturating_add(n);
        self.seek(target)
    }

    pub fn read_u8(&mut self) -> Result<u8> {
        Ok(self.cur.read_u8()?)
    }

    pub fn read_i8(&mut self) -> Result<i8> {
        Ok(self.cur.read_i8()?)
    }

    pub fn read_u16(&mut self) -> Result<u16> {
        Ok(self.cur.read_u16::<LittleEndian>()?)
    }

    pub fn read_i16(&mut self) -> Result<i16> {
        Ok(self.cur.read_i16::<LittleEndian>()?)
    }

    pub fn read_u32(&mut self) -> Result<u32> {
        Ok(self.cur.read_u32::<LittleEndian>()?)
    }

    pub fn read_i32(&mut self) -> Result<i32> {
        Ok(self.cur.read_i32::<LittleEndian>()?)
    }

    pub fn read_u64(&mut self) -> Result<u64> {
        Ok(self.cur.read_u64::<LittleEndian>()?)
    }

    pub fn read_i64(&mut self) -> Result<i64> {
        Ok(self.cur.read_i64::<LittleEndian>()?)
    }

    pub fn read_f32(&mut self) -> Result<f32> {
        Ok(self.cur.read_f32::<LittleEndian>()?)
    }

    pub fn read_f64(&mut self) -> Result<f64> {
        Ok(self.cur.read_f64::<LittleEndian>()?)
    }

    pub fn read_bool32(&mut self) -> Result<bool> {
        Ok(self.read_i32()? != 0)
    }

    pub fn read_bytes(&mut self, n: usize) -> Result<Vec<u8>> {
        if n as u64 > self.remaining() {
            bail!(
                "read {} bytes out of range, only {} bytes remaining",
                n,
                self.remaining()
            );
        }
        let mut buf = vec![0u8; n];
        self.cur.read_exact(&mut buf)?;
        Ok(buf)
    }

    pub fn read_guid(&mut self) -> Result<Guid> {
        Ok(Guid([
            self.read_u32()?,
            self.read_u32()?,
            self.read_u32()?,
            self.read_u32()?,
        ]))
    }

    pub fn read_io_hash(&mut self) -> Result<[u8; 20]> {
        let bytes = self.read_bytes(20)?;
        let mut b = [0u8; 20];
        b.copy_from_slice(&bytes);
        Ok(b)
    }

    pub fn read_raw_name(&mut self) -> Result<RawName> {
        Ok(RawName {
            index: self.read_i32()?,
            number: self.read_i32()?,
        })
    }

    /// Fails unless `bytes` more bytes fit inside `end`.
    ///
    /// Decoders working inside a bounded window (an export payload, a declared
    /// tagged-property range, a property value) must check every read against
    /// that window rather than against the end of the file, or a variable-length
    /// field near the boundary silently reads unrelated bytes.
    pub fn ensure_within(&self, end: u64, bytes: u64, what: &str) -> Result<()> {
        let pos = self.pos();
        if pos.saturating_add(bytes) > end {
            bail!("{what} at byte {pos} needs {bytes} byte(s) but the window ends at {end}");
        }
        Ok(())
    }

    pub fn read_u8_within(&mut self, end: u64, what: &str) -> Result<u8> {
        self.ensure_within(end, 1, what)?;
        self.read_u8()
    }

    pub fn read_i32_within(&mut self, end: u64, what: &str) -> Result<i32> {
        self.ensure_within(end, 4, what)?;
        self.read_i32()
    }

    pub fn read_bool32_within(&mut self, end: u64, what: &str) -> Result<bool> {
        Ok(self.read_i32_within(end, what)? != 0)
    }

    pub fn read_guid_within(&mut self, end: u64, what: &str) -> Result<Guid> {
        self.ensure_within(end, 16, what)?;
        self.read_guid()
    }

    pub fn read_raw_name_within(&mut self, end: u64, what: &str) -> Result<RawName> {
        self.ensure_within(end, RAW_NAME_BYTES, what)?;
        self.read_raw_name()
    }

    /// An `FString` whose length prefix *and* payload must both fit inside `end`.
    /// A negative length counts UTF-16 code units, so the payload is twice as long.
    pub fn read_fstring_within(&mut self, end: u64, what: &str) -> Result<String> {
        self.ensure_within(end, 4, what)?;
        let len = self.read_i32()?;
        let payload = if len >= 0 {
            len as u64
        } else {
            (len.unsigned_abs() as u64).saturating_mul(2)
        };
        self.ensure_within(end, payload, what)?;
        let value = self.read_fstring_of_length(len)?;
        Ok(value)
    }

    pub fn read_fstring(&mut self) -> Result<String> {
        let len = self.read_i32()?;
        self.read_fstring_of_length(len)
    }

    fn read_fstring_of_length(&mut self, len: i32) -> Result<String> {
        if len == 0 {
            return Ok(String::new());
        }
        if len > 0 {
            let n = len as usize;
            if n as u64 > self.remaining() {
                bail!("FString (ANSI) length out of range: {}", len);
            }
            let mut buf = self.read_bytes(n)?;
            if buf.last() == Some(&0) {
                buf.pop();
            }
            Ok(String::from_utf8_lossy(&buf).into_owned())
        } else {
            let n = len.unsigned_abs() as usize;
            if (n as u64).saturating_mul(2) > self.remaining() {
                bail!("FString (UTF-16) length out of range: {}", len);
            }
            let mut units = Vec::with_capacity(n);
            for _ in 0..n {
                units.push(self.read_u16()?);
            }
            if units.last() == Some(&0) {
                units.pop();
            }
            Ok(String::from_utf16_lossy(&units))
        }
    }
}
