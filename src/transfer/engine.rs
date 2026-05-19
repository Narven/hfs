use anyhow::Result;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::backend::Backend;
use crate::cas::Store;
use crate::manifest::Manifest;

const DEFAULT_CONCURRENCY: usize = 32;

pub struct TransferEngine {
    store: Store,
    backend: Arc<dyn Backend>,
    concurrency: usize,
}

impl TransferEngine {
    pub fn new(store: Store, backend: Arc<dyn Backend>) -> Self {
        Self {
            store,
            backend,
            concurrency: DEFAULT_CONCURRENCY,
        }
    }

    pub fn with_concurrency(mut self, n: usize) -> Self {
        self.concurrency = n;
        self
    }

    /// Push all chunks referenced by the given manifests to the remote.
    /// Returns (pushed_count, skipped_count).
    pub async fn push(&self, manifest_hashes: &[[u8; 32]]) -> Result<(usize, usize)> {
        let chunk_hashes = self.collect_chunk_hashes(manifest_hashes)?;

        let sem = Arc::new(Semaphore::new(self.concurrency));
        let mut handles = Vec::new();
        let mut skipped = 0;

        for hash in chunk_hashes {
            let has_remote = self.backend.has_chunk(&hash).await?;
            if has_remote {
                skipped += 1;
                continue;
            }

            let data = self.store.get_object(&hash)?;
            let backend = Arc::clone(&self.backend);
            let permit = Arc::clone(&sem);

            handles.push(tokio::spawn(async move {
                let _permit = permit.acquire().await.unwrap();
                backend.push_chunk(&hash, &data).await
            }));
        }

        let mut pushed = 0;
        for handle in handles {
            handle.await??;
            pushed += 1;
        }

        // Also push manifests
        for mh in manifest_hashes {
            let data = self.store.get_manifest(mh)?;
            self.backend.push_chunk(mh, &data).await?;
        }

        Ok((pushed, skipped))
    }

    /// Pull all chunks referenced by the given manifests from the remote.
    /// Returns (pulled_count, skipped_count).
    pub async fn pull(&self, manifest_hashes: &[[u8; 32]]) -> Result<(usize, usize)> {
        // First pull the manifests themselves
        for mh in manifest_hashes {
            if !self.store.has_manifest(mh) {
                let data = self.backend.pull_chunk(mh).await?;
                self.store.put_manifest(mh, &data)?;
            }
        }

        let chunk_hashes = self.collect_chunk_hashes(manifest_hashes)?;

        let sem = Arc::new(Semaphore::new(self.concurrency));
        let mut handles = Vec::new();
        let mut skipped = 0;

        for hash in chunk_hashes {
            if self.store.has_object(&hash) {
                skipped += 1;
                continue;
            }

            let backend = Arc::clone(&self.backend);
            let permit = Arc::clone(&sem);

            handles.push(tokio::spawn(async move {
                let _permit = permit.acquire().await.unwrap();
                let data = backend.pull_chunk(&hash).await?;
                Ok::<_, anyhow::Error>((hash, data))
            }));
        }

        let mut pulled = 0;
        for handle in handles {
            let (hash, data) = handle.await??;
            self.store.put_object(&hash, &data)?;
            pulled += 1;
        }

        Ok((pulled, skipped))
    }

    fn collect_chunk_hashes(&self, manifest_hashes: &[[u8; 32]]) -> Result<Vec<[u8; 32]>> {
        let mut seen = HashSet::new();
        let mut result = Vec::new();

        for mh in manifest_hashes {
            let manifest_bytes = self.store.get_manifest(mh)?;
            let manifest = Manifest::deserialize(&manifest_bytes)?;
            for chunk_ref in &manifest.chunks {
                if seen.insert(chunk_ref.hash) {
                    result.push(chunk_ref.hash);
                }
            }
        }

        Ok(result)
    }
}
