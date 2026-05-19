use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u8,
    pub file_size: u64,
    pub chunk_size_avg: u32,
    pub chunks: Vec<ChunkRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkRef {
    pub hash: [u8; 32],
    pub offset: u64,
    pub size: u32,
    pub original_size: u32,
}

impl Manifest {
    pub fn serialize(&self) -> Result<Vec<u8>> {
        Ok(rmp_serde::to_vec(self)?)
    }

    pub fn deserialize(data: &[u8]) -> Result<Self> {
        Ok(rmp_serde::from_slice(data)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let manifest = Manifest {
            version: 1,
            file_size: 1024,
            chunk_size_avg: 512,
            chunks: vec![
                ChunkRef {
                    hash: [1u8; 32],
                    offset: 0,
                    size: 100,
                    original_size: 512,
                },
                ChunkRef {
                    hash: [2u8; 32],
                    offset: 512,
                    size: 100,
                    original_size: 512,
                },
            ],
        };

        let bytes = manifest.serialize().unwrap();
        let recovered = Manifest::deserialize(&bytes).unwrap();

        assert_eq!(recovered.version, 1);
        assert_eq!(recovered.file_size, 1024);
        assert_eq!(recovered.chunks.len(), 2);
        assert_eq!(recovered.chunks[0].hash, [1u8; 32]);
        assert_eq!(recovered.chunks[1].offset, 512);
    }

    #[test]
    fn empty_manifest() {
        let manifest = Manifest {
            version: 1,
            file_size: 0,
            chunk_size_avg: 0,
            chunks: vec![],
        };
        let bytes = manifest.serialize().unwrap();
        let recovered = Manifest::deserialize(&bytes).unwrap();
        assert!(recovered.chunks.is_empty());
        assert_eq!(recovered.file_size, 0);
    }
}
