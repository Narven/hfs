use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;

use crate::cas::Store;
use crate::cas::hash::hash_to_hex;
use crate::config::Config;
use crate::manifest::Manifest;

pub fn run(cwd: &Path, dry_run: bool) -> Result<()> {
    let hfs_dir = Config::find_hfs_dir(cwd)
        .ok_or_else(|| anyhow::anyhow!("not an HFS repository (no .hfs directory found)"))?;

    let store = Store::new(&hfs_dir);

    // Collect all referenced chunk hashes from all manifests
    let manifests = store.list_manifests()?;
    let mut referenced_chunks: HashSet<[u8; 32]> = HashSet::new();

    for mh in &manifests {
        match store.get_manifest(mh) {
            Ok(data) => {
                if let Ok(manifest) = Manifest::deserialize(&data) {
                    for chunk_ref in &manifest.chunks {
                        referenced_chunks.insert(chunk_ref.hash);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("could not read manifest {}: {e}", hash_to_hex(mh));
            }
        }
    }

    let all_objects = store.list_objects()?;
    let mut orphaned = Vec::new();

    for hash in &all_objects {
        if !referenced_chunks.contains(hash) {
            orphaned.push(*hash);
        }
    }

    if orphaned.is_empty() {
        println!("No orphaned objects to clean up.");
        return Ok(());
    }

    println!(
        "Found {} orphaned objects out of {} total.",
        orphaned.len(),
        all_objects.len()
    );

    if dry_run {
        for hash in &orphaned {
            println!("  would remove: {}", hash_to_hex(hash));
        }
        println!("Dry run -- no objects removed. Run without --dry-run to delete.");
    } else {
        for hash in &orphaned {
            store.remove_object(hash)?;
            println!("  removed: {}", hash_to_hex(hash));
        }
        println!("Removed {} orphaned objects.", orphaned.len());
    }

    Ok(())
}
