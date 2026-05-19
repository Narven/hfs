use anyhow::Result;
use std::path::Path;
use std::sync::Arc;

use crate::backend::s3::S3Backend;
use crate::cas::Store;
use crate::config::Config;
use crate::pointer::Pointer;
use crate::transfer::engine::TransferEngine;

pub async fn run(cwd: &Path) -> Result<()> {
    let hfs_dir = Config::find_hfs_dir(cwd)
        .ok_or_else(|| anyhow::anyhow!("not an HFS repository (no .hfs directory found)"))?;

    let config = Config::load(&hfs_dir)?;
    let _store = Store::new(&hfs_dir);

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

    // Scan working directory for pointer files and collect their manifest hashes
    let manifest_hashes = collect_pointer_manifest_hashes(cwd)?;

    if manifest_hashes.is_empty() {
        println!("No HFS pointer files found. Nothing to pull.");
        return Ok(());
    }

    println!("Pulling chunks for {} pointer file(s)...", manifest_hashes.len());

    let engine = TransferEngine::new(Store::new(&hfs_dir), backend);
    let (pulled, skipped) = engine.pull(&manifest_hashes).await?;

    println!("Done: {pulled} chunks pulled, {skipped} already present locally.");

    Ok(())
}

fn collect_pointer_manifest_hashes(cwd: &Path) -> Result<Vec<[u8; 32]>> {
    let mut hashes = Vec::new();
    collect_pointers_recursive(cwd, cwd, &mut hashes)?;
    Ok(hashes)
}

fn collect_pointers_recursive(root: &Path, dir: &Path, hashes: &mut Vec<[u8; 32]>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        // Skip hidden directories
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') {
                continue;
            }
        }

        if path.is_dir() {
            collect_pointers_recursive(root, &path, hashes)?;
        } else if path.is_file() {
            if let Ok(data) = std::fs::read(&path) {
                if Pointer::is_pointer(&data) {
                    if let Ok(text) = std::str::from_utf8(&data) {
                        if let Ok(ptr) = Pointer::decode(text) {
                            hashes.push(ptr.oid);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
