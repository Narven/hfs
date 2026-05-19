#![allow(dead_code)]

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::path::{Path, PathBuf};

use hfs::cas::Store;

pub const SEED: u64 = 42;

pub const SIZE_1MB: usize = 1 << 20;
pub const SIZE_10MB: usize = 10 << 20;
pub const SIZE_100MB: usize = 100 << 20;

/// Generate reproducible pseudorandom data with some compressible structure.
/// Mixes random bytes with repeated blocks to simulate real binary assets.
pub fn generate_data(size: usize, seed: u64) -> Vec<u8> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut data = vec![0u8; size];

    // Fill 70% random, 30% repeated patterns (realistic for binary assets)
    let random_cutoff = size * 7 / 10;
    rng.fill(&mut data[..random_cutoff]);

    if random_cutoff < size {
        let pattern_len = 4096.min(size - random_cutoff);
        let pattern: Vec<u8> = (0..pattern_len).map(|_| rng.r#gen::<u8>()).collect();
        for chunk in data[random_cutoff..].chunks_mut(pattern_len) {
            let copy_len = chunk.len().min(pattern_len);
            chunk[..copy_len].copy_from_slice(&pattern[..copy_len]);
        }
    }

    data
}

/// Apply a contiguous edit to `edit_pct` of the data at a random offset.
pub fn apply_edit(data: &mut [u8], edit_pct: f64, seed: u64) {
    let mut rng = StdRng::seed_from_u64(seed);
    let edit_bytes = ((data.len() as f64) * edit_pct).max(1.0) as usize;
    let max_start = data.len().saturating_sub(edit_bytes);
    let start = if max_start == 0 {
        0
    } else {
        rng.gen_range(0..max_start)
    };
    rng.fill(&mut data[start..start + edit_bytes]);
}

/// Create a temp directory with an initialized HFS Store.
pub fn temp_store(dir: &Path) -> Store {
    let hfs_dir = dir.join(".hfs");
    let store = Store::new(&hfs_dir);
    store.init().unwrap();
    store
}

/// Total bytes on disk in the objects/ subdirectory of a store.
pub fn store_objects_size(store: &Store) -> u64 {
    dir_size(&store.root().join("objects"))
}

/// Total bytes on disk in the manifests/ subdirectory of a store.
pub fn store_manifests_size(store: &Store) -> u64 {
    dir_size(&store.root().join("manifests"))
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let ft = entry.file_type().unwrap();
            if ft.is_dir() {
                total += dir_size(&entry.path());
            } else {
                total += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    total
}

/// Human-readable byte size.
pub fn human_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Initialize a git repo at the given path, returning the path.
pub fn init_git_repo(path: &Path) -> PathBuf {
    std::fs::create_dir_all(path).unwrap();
    run_cmd(path, "git", &["init", "-q"]);
    run_cmd(path, "git", &["config", "user.email", "bench@test.com"]);
    run_cmd(path, "git", &["config", "user.name", "Benchmark"]);
    path.to_path_buf()
}

/// Run a command, panic on failure.
pub fn run_cmd(cwd: &Path, cmd: &str, args: &[&str]) -> String {
    let output = std::process::Command::new(cmd)
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|e| panic!("failed to run {cmd} {}: {e}", args.join(" ")));
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("{cmd} {} failed: {stderr}", args.join(" "));
    }
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Check if a command is available on PATH.
pub fn command_exists(cmd: &str) -> bool {
    std::process::Command::new(cmd)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}
