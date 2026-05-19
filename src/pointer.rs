use anyhow::{Context, Result, bail};

use crate::cas::hash::{hash_to_hex, hex_to_hash};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pointer {
    pub version: u8,
    pub oid: [u8; 32],
    pub size: u64,
    pub chunk_count: u32,
}

const HEADER: &str = "hfs v1";
const MAX_POINTER_SIZE: usize = 256;

impl Pointer {
    pub fn encode(&self) -> String {
        format!(
            "{HEADER}\noid blake3:{}\nsize {}\nchunks {}\n",
            hash_to_hex(&self.oid),
            self.size,
            self.chunk_count,
        )
    }

    pub fn decode(s: &str) -> Result<Self> {
        let mut lines = s.lines();

        let header = lines.next().context("missing header")?;
        if header != HEADER {
            bail!("invalid hfs pointer header: {header:?}");
        }

        let mut oid: Option<[u8; 32]> = None;
        let mut size: Option<u64> = None;
        let mut chunk_count: Option<u32> = None;

        for line in lines {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(rest) = line.strip_prefix("oid blake3:") {
                oid = Some(hex_to_hash(rest).context("invalid oid")?);
            } else if let Some(rest) = line.strip_prefix("size ") {
                size = Some(rest.parse().context("invalid size")?);
            } else if let Some(rest) = line.strip_prefix("chunks ") {
                chunk_count = Some(rest.parse().context("invalid chunk count")?);
            }
        }

        Ok(Pointer {
            version: 1,
            oid: oid.context("missing oid")?,
            size: size.context("missing size")?,
            chunk_count: chunk_count.context("missing chunk count")?,
        })
    }

    /// Quick check: does this look like an HFS pointer?
    pub fn is_pointer(data: &[u8]) -> bool {
        if data.len() > MAX_POINTER_SIZE {
            return false;
        }
        data.starts_with(HEADER.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let ptr = Pointer {
            version: 1,
            oid: [0xAB; 32],
            size: 123456,
            chunk_count: 5,
        };
        let encoded = ptr.encode();
        let decoded = Pointer::decode(&encoded).unwrap();
        assert_eq!(ptr, decoded);
    }

    #[test]
    fn is_pointer_positive() {
        let ptr = Pointer {
            version: 1,
            oid: [0; 32],
            size: 0,
            chunk_count: 0,
        };
        let encoded = ptr.encode();
        assert!(Pointer::is_pointer(encoded.as_bytes()));
    }

    #[test]
    fn is_pointer_negative() {
        assert!(!Pointer::is_pointer(b"not a pointer at all"));
        assert!(!Pointer::is_pointer(&vec![0u8; 1024]));
    }

    #[test]
    fn decode_rejects_bad_header() {
        assert!(Pointer::decode("git-lfs v1\noid sha256:abc\n").is_err());
    }
}
