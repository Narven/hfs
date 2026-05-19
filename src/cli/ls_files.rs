use anyhow::Result;
use std::path::Path;
use std::process::Command;

use crate::pointer::Pointer;

pub fn run(cwd: &Path) -> Result<()> {
    // Use git ls-files to find files tracked by the hfs filter
    let output = Command::new("git")
        .args(["ls-files"])
        .current_dir(cwd)
        .output()?;

    if !output.status.success() {
        anyhow::bail!("git ls-files failed");
    }

    let file_list = String::from_utf8(output.stdout)?;
    let mut found = false;

    for filename in file_list.lines() {
        let filepath = cwd.join(filename);
        if !filepath.exists() {
            continue;
        }

        // Check if .gitattributes marks this file with filter=hfs
        if !is_hfs_tracked(cwd, filename)? {
            continue;
        }

        let metadata = std::fs::metadata(&filepath)?;
        let size = metadata.len();

        // Check if the working copy is a pointer or a materialized file
        let data = std::fs::read(&filepath)?;
        let state = if Pointer::is_pointer(&data) {
            "pointer"
        } else {
            "file"
        };

        println!("{:>10}  {:<8}  {}", format_bytes(size), state, filename);
        found = true;
    }

    if !found {
        println!("No HFS-tracked files found.");
    }

    Ok(())
}

fn is_hfs_tracked(cwd: &Path, filename: &str) -> Result<bool> {
    let output = Command::new("git")
        .args(["check-attr", "filter", "--", filename])
        .current_dir(cwd)
        .output()?;

    let stdout = String::from_utf8(output.stdout)?;
    Ok(stdout.contains("hfs"))
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
