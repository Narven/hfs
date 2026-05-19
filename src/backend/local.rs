use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::{Path, PathBuf};

use crate::cas::hash::{hash_to_hex, hex_to_hash};
use super::Backend;

/// A local filesystem backend (for testing or local-network shared storage).
pub struct LocalBackend {
    root: PathBuf,
}

impl LocalBackend {
    pub fn new(root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root)?;
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    fn chunk_path(&self, hash: &[u8; 32]) -> PathBuf {
        let hex = hash_to_hex(hash);
        let (prefix, rest) = hex.split_at(2);
        self.root.join(prefix).join(rest)
    }
}

#[async_trait]
impl Backend for LocalBackend {
    async fn push_chunk(&self, hash: &[u8; 32], data: &[u8]) -> Result<()> {
        let path = self.chunk_path(hash);
        if path.exists() {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, data).context("writing chunk to local backend")?;
        Ok(())
    }

    async fn pull_chunk(&self, hash: &[u8; 32]) -> Result<Vec<u8>> {
        let path = self.chunk_path(hash);
        std::fs::read(&path).with_context(|| format!("reading chunk {}", hash_to_hex(hash)))
    }

    async fn has_chunk(&self, hash: &[u8; 32]) -> Result<bool> {
        Ok(self.chunk_path(hash).exists())
    }

    async fn list_chunks(&self) -> Result<Vec<[u8; 32]>> {
        let mut hashes = Vec::new();
        if !self.root.exists() {
            return Ok(hashes);
        }
        for prefix_entry in std::fs::read_dir(&self.root)? {
            let prefix_entry = prefix_entry?;
            if !prefix_entry.file_type()?.is_dir() {
                continue;
            }
            let prefix = prefix_entry.file_name().to_string_lossy().to_string();
            for file_entry in std::fs::read_dir(prefix_entry.path())? {
                let file_entry = file_entry?;
                let rest = file_entry.file_name().to_string_lossy().to_string();
                let hex = format!("{prefix}{rest}");
                if let Ok(hash) = hex_to_hash(&hex) {
                    hashes.push(hash);
                }
            }
        }
        Ok(hashes)
    }
}
