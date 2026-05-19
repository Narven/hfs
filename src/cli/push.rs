use anyhow::Result;
use std::path::Path;
use std::sync::Arc;

use crate::backend::s3::S3Backend;
use crate::cas::Store;
use crate::config::Config;
use crate::transfer::engine::TransferEngine;

pub async fn run(cwd: &Path) -> Result<()> {
    let hfs_dir = Config::find_hfs_dir(cwd)
        .ok_or_else(|| anyhow::anyhow!("not an HFS repository (no .hfs directory found)"))?;

    let config = Config::load(&hfs_dir)?;
    let store = Store::new(&hfs_dir);

    let remote = config.remote.as_ref()
        .ok_or_else(|| anyhow::anyhow!("no remote configured in .hfs/config.toml"))?;

    let backend: Arc<dyn crate::backend::Backend> = match remote.backend.as_str() {
        "s3" => {
            let bucket = remote.bucket.clone()
                .ok_or_else(|| anyhow::anyhow!("S3 backend requires 'bucket' in config"))?;
            Arc::new(S3Backend::new(
                bucket,
                remote.prefix.clone(),
                remote.region.clone(),
                remote.endpoint.clone(),
            ).await?)
        }
        other => anyhow::bail!("unsupported backend: {other}"),
    };

    let manifests = store.list_manifests()?;
    if manifests.is_empty() {
        println!("Nothing to push.");
        return Ok(());
    }

    println!("Pushing {} manifest(s) and their chunks...", manifests.len());

    let engine = TransferEngine::new(Store::new(&hfs_dir), backend);
    let (pushed, skipped) = engine.push(&manifests).await?;

    println!("Done: {pushed} chunks pushed, {skipped} already present on remote.");

    Ok(())
}
