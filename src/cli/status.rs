use anyhow::Result;
use std::path::Path;

use crate::cas::Store;
use crate::config::Config;
use crate::manifest::Manifest;

pub fn run(cwd: &Path) -> Result<()> {
    let hfs_dir = Config::find_hfs_dir(cwd)
        .ok_or_else(|| anyhow::anyhow!("not an HFS repository (no .hfs directory found)"))?;

    let store = Store::new(&hfs_dir);

    let objects = store.list_objects()?;
    let manifests = store.list_manifests()?;

    let total_object_bytes: u64 = objects
        .iter()
        .filter_map(|h| store.get_object(h).ok().map(|d| d.len() as u64))
        .sum();

    println!("HFS status:");
    println!("  Store:      {}", hfs_dir.display());
    println!(
        "  Objects:    {} ({} compressed)",
        objects.len(),
        format_bytes(total_object_bytes)
    );
    println!("  Manifests:  {}", manifests.len());

    // List tracked files by scanning .gitattributes
    let gitattributes_path = cwd.join(".gitattributes");
    if gitattributes_path.exists() {
        let content = std::fs::read_to_string(&gitattributes_path)?;
        let patterns: Vec<&str> = content
            .lines()
            .filter(|l| l.contains("filter=hfs"))
            .filter_map(|l| l.split_whitespace().next())
            .collect();

        if !patterns.is_empty() {
            println!("  Tracked patterns:");
            for p in &patterns {
                println!("    {p}");
            }
        }
    }

    // Show manifest details
    if !manifests.is_empty() {
        println!("\n  Stored files:");
        for mh in &manifests {
            if let Ok(data) = store.get_manifest(mh)
                && let Ok(m) = Manifest::deserialize(&data) {
                    println!(
                        "    {} ({}, {} chunks)",
                        crate::cas::hash::hash_to_hex(mh),
                        format_bytes(m.file_size),
                        m.chunks.len(),
                    );
                }
        }
    }

    Ok(())
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}
