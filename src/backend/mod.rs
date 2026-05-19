pub mod local;
pub mod s3;

use anyhow::Result;
use async_trait::async_trait;

/// Trait for remote storage backends.
#[async_trait]
pub trait Backend: Send + Sync {
    async fn push_chunk(&self, hash: &[u8; 32], data: &[u8]) -> Result<()>;
    async fn pull_chunk(&self, hash: &[u8; 32]) -> Result<Vec<u8>>;
    async fn has_chunk(&self, hash: &[u8; 32]) -> Result<bool>;
    async fn list_chunks(&self) -> Result<Vec<[u8; 32]>>;
}
