use anyhow::{Result, Context};
use std::path::Path;
use std::process::Command;

use crate::cas::Store;
use crate::config::Config;

pub fn run(cwd: &Path) -> Result<()> {
    let hfs_dir = cwd.join(".hfs");

    if hfs_dir.exists() {
        println!("HFS already initialized in {}", hfs_dir.display());
        return Ok(());
    }

    let store = Store::new(&hfs_dir);
    store.init()?;

    let config = Config::default();
    config.save(&hfs_dir)?;

    // Configure git filter
    git_config(cwd, "filter.hfs.process", "hfs filter-process")?;
    git_config(cwd, "filter.hfs.required", "true")?;

    // Add .hfs/ to .gitignore if not already there
    add_to_gitignore(cwd, ".hfs/")?;

    println!("Initialized HFS in {}", hfs_dir.display());
    println!("Run `hfs track \"*.bin\"` to start tracking large files.");

    Ok(())
}

fn git_config(cwd: &Path, key: &str, value: &str) -> Result<()> {
    let status = Command::new("git")
        .args(["config", "--local", key, value])
        .current_dir(cwd)
        .status()
        .context("running git config")?;

    if !status.success() {
        anyhow::bail!("git config failed for {key}={value}");
    }
    Ok(())
}

fn add_to_gitignore(cwd: &Path, entry: &str) -> Result<()> {
    let gitignore_path = cwd.join(".gitignore");
    let content = if gitignore_path.exists() {
        std::fs::read_to_string(&gitignore_path)?
    } else {
        String::new()
    };

    if content.lines().any(|line| line.trim() == entry) {
        return Ok(());
    }

    let mut new_content = content;
    if !new_content.is_empty() && !new_content.ends_with('\n') {
        new_content.push('\n');
    }
    new_content.push_str(entry);
    new_content.push('\n');

    std::fs::write(&gitignore_path, new_content)?;
    Ok(())
}
