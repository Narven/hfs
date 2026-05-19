use anyhow::Result;
use std::path::Path;
use std::sync::Arc;

use crate::backend::s3::S3Backend;
use crate::cas::Store;
use crate::config::Config;
use crate::manifest::Manifest;
use crate::pointer::Pointer;
use crate::transfer::engine::TransferEngine;

/// After `git clone`, fetch all chunks referenced by pointer files in the working tree.
pub async fn run(cwd: &Path) -> Result<()> {
    let hfs_dir = Config::find_hfs_dir(cwd)
        .ok_or_else(|| anyhow::anyhow!("not an HFS repository (no .hfs directory found)"))?;

    let config = Config::load(&hfs_dir)?;
    let store = Store::new(&hfs_dir);

    let remote = config
        .remote
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no remote configured in .hfs/config.toml"))?;

    let backend: Arc<dyn crate::backend::Backend> = match remote.backend.as_str() {
        "s3" => {
            let bucket = remote
                .bucket
                .clone()
                .ok_or_else(|| anyhow::anyhow!("S3 backend requires 'bucket' in config"))?;
            Arc::new(
                S3Backend::new(
                    bucket,
                    remote.prefix.clone(),
                    remote.region.clone(),
                    remote.endpoint.clone(),
                )
                .await?,
            )
        }
        other => anyhow::bail!("unsupported backend: {other}"),
    };

    let manifest_hashes = collect_all_pointer_manifests(cwd)?;

    if manifest_hashes.is_empty() {
        println!("No HFS pointer files found.");
        return Ok(());
    }

    println!("Fetching chunks for {} file(s)...", manifest_hashes.len());

    let engine = TransferEngine::new(Store::new(&hfs_dir), backend);
    let (pulled, skipped) = engine.pull(&manifest_hashes).await?;

    println!("Done: {pulled} chunks fetched, {skipped} already cached.");

    // Now verify all files can be materialized
    let mut ok = 0;
    let mut err = 0;
    for mh in &manifest_hashes {
        let manifest_bytes = store.get_manifest(mh)?;
        let manifest = Manifest::deserialize(&manifest_bytes)?;
        let all_present = manifest.chunks.iter().all(|c| store.has_object(&c.hash));
        if all_present {
            ok += 1;
        } else {
            err += 1;
        }
    }

    if err > 0 {
        println!("Warning: {err} file(s) still have missing chunks.");
    }
    println!("{ok} file(s) ready.");

    Ok(())
}

fn collect_all_pointer_manifests(cwd: &Path) -> Result<Vec<[u8; 32]>> {
    let mut hashes = Vec::new();
    collect_recursive(cwd, &mut hashes)?;
    Ok(hashes)
}

fn collect_recursive(dir: &Path, hashes: &mut Vec<[u8; 32]>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if let Some(name) = path.file_name().and_then(|n| n.to_str())
            && name.starts_with('.') {
                continue;
            }

        if path.is_dir() {
            collect_recursive(&path, hashes)?;
        } else if path.is_file()
            && let Ok(data) = std::fs::read(&path)
                && Pointer::is_pointer(&data)
                    && let Ok(text) = std::str::from_utf8(&data)
                        && let Ok(ptr) = Pointer::decode(text) {
                            hashes.push(ptr.oid);
                        }
    }
    Ok(())
}
