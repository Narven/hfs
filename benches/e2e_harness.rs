mod utils;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::json;

// ---------------------------------------------------------------------------
// End-to-end benchmark: HFS vs git-lfs
//
// Runs real git commands with each tool and compares wall-clock times.
// Requires `git` on PATH; git-lfs scenarios are skipped if `git-lfs` is
// not available.
// ---------------------------------------------------------------------------

const ITERATIONS: usize = 3;

const FILE_SIZES: &[(usize, &str)] = &[(10 << 20, "10 MB"), (100 << 20, "100 MB")];

const MULTI_FILE_COUNT: usize = 100;
const MULTI_FILE_SIZE: usize = 1 << 20; // 1 MB each

// ---------------------------------------------------------------------------
// Timing helpers
// ---------------------------------------------------------------------------

struct TimingResult {
    median: Duration,
    #[allow(dead_code)]
    all: Vec<Duration>,
}

fn time_iterations(iterations: usize, mut f: impl FnMut()) -> TimingResult {
    let mut all = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        f();
        all.push(start.elapsed());
    }
    all.sort();
    let median = all[all.len() / 2];
    TimingResult { median, all }
}

fn fmt_duration(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs >= 1.0 {
        format!("{:.2}s", secs)
    } else {
        format!("{:.0}ms", secs * 1000.0)
    }
}

// ---------------------------------------------------------------------------
// HFS repo helpers
// ---------------------------------------------------------------------------

fn hfs_binary() -> PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop(); // remove bench binary name
    path.pop(); // remove deps/
    path.push("hfs");
    if cfg!(windows) {
        path.set_extension("exe");
    }
    if !path.exists() {
        // Fallback: try release build location
        let mut release = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        release.push("target");
        release.push("release");
        release.push("hfs");
        if cfg!(windows) {
            release.set_extension("exe");
        }
        return release;
    }
    path
}

fn setup_hfs_repo(root: &Path, name: &str) -> PathBuf {
    let repo = root.join(name);
    utils::init_git_repo(&repo);
    let hfs = hfs_binary();
    let hfs_str = hfs.to_str().unwrap();
    utils::run_cmd(&repo, hfs_str, &["init"]);
    utils::run_cmd(&repo, hfs_str, &["track", "*.bin"]);
    utils::run_cmd(&repo, "git", &["add", ".gitattributes", ".gitignore"]);
    utils::run_cmd(&repo, "git", &["commit", "-q", "-m", "init hfs"]);
    repo
}

fn setup_lfs_repo(root: &Path, name: &str) -> PathBuf {
    let repo = root.join(name);
    utils::init_git_repo(&repo);
    utils::run_cmd(&repo, "git", &["lfs", "install", "--local"]);
    utils::run_cmd(&repo, "git", &["lfs", "track", "*.bin"]);
    utils::run_cmd(&repo, "git", &["add", ".gitattributes"]);
    utils::run_cmd(&repo, "git", &["commit", "-q", "-m", "init lfs"]);
    repo
}

fn write_bin_file(repo: &Path, name: &str, data: &[u8]) {
    std::fs::write(repo.join(name), data).unwrap();
}

fn git_add_commit(repo: &Path, msg: &str) {
    utils::run_cmd(repo, "git", &["add", "."]);
    utils::run_cmd(repo, "git", &["commit", "-q", "-m", msg]);
}

// ---------------------------------------------------------------------------
// Scenario: add + commit
// ---------------------------------------------------------------------------

struct ScenarioResult {
    hfs_time: Duration,
    lfs_time: Option<Duration>,
    speedup: Option<f64>,
    extra: String,
}

fn scenario_add_commit(root: &Path, data: &[u8], has_lfs: bool) -> ScenarioResult {
    let hfs_result = time_iterations(ITERATIONS, || {
        let repo = setup_hfs_repo(root, "hfs-add");
        write_bin_file(&repo, "large.bin", data);
        git_add_commit(&repo, "add large file");
        // Clean up for next iteration
        let _ = std::fs::remove_dir_all(&repo);
    });

    let lfs_result = if has_lfs {
        Some(time_iterations(ITERATIONS, || {
            let repo = setup_lfs_repo(root, "lfs-add");
            write_bin_file(&repo, "large.bin", data);
            git_add_commit(&repo, "add large file");
            let _ = std::fs::remove_dir_all(&repo);
        }))
    } else {
        None
    };

    let speedup = lfs_result
        .as_ref()
        .map(|lfs| lfs.median.as_secs_f64() / hfs_result.median.as_secs_f64());

    ScenarioResult {
        hfs_time: hfs_result.median,
        lfs_time: lfs_result.map(|r| r.median),
        speedup,
        extra: String::new(),
    }
}

// ---------------------------------------------------------------------------
// Scenario: version edit (dedup advantage)
// ---------------------------------------------------------------------------

fn scenario_version_edit(root: &Path, base_data: &[u8], has_lfs: bool) -> ScenarioResult {
    let size = base_data.len();
    let mut edited = base_data.to_vec();
    utils::apply_edit(&mut edited, 0.01, utils::SEED + 99);

    let hfs_result = time_iterations(ITERATIONS, || {
        let repo = setup_hfs_repo(root, "hfs-vedit");
        write_bin_file(&repo, "asset.bin", base_data);
        git_add_commit(&repo, "v1");
        write_bin_file(&repo, "asset.bin", &edited);
        git_add_commit(&repo, "v2");
        let _ = std::fs::remove_dir_all(&repo);
    });

    let lfs_result = if has_lfs {
        Some(time_iterations(ITERATIONS, || {
            let repo = setup_lfs_repo(root, "lfs-vedit");
            write_bin_file(&repo, "asset.bin", base_data);
            git_add_commit(&repo, "v1");
            write_bin_file(&repo, "asset.bin", &edited);
            git_add_commit(&repo, "v2");
            let _ = std::fs::remove_dir_all(&repo);
        }))
    } else {
        None
    };

    // Measure storage after the two commits for hfs
    let hfs_repo = setup_hfs_repo(root, "hfs-vedit-measure");
    write_bin_file(&hfs_repo, "asset.bin", base_data);
    git_add_commit(&hfs_repo, "v1");
    write_bin_file(&hfs_repo, "asset.bin", &edited);
    git_add_commit(&hfs_repo, "v2");

    let hfs_store = hfs::cas::Store::new(&hfs_repo.join(".hfs"));
    let hfs_stored =
        utils::store_objects_size(&hfs_store) + utils::store_manifests_size(&hfs_store);
    let lfs_stored = (size as u64) * 2;
    let savings = (1.0 - hfs_stored as f64 / lfs_stored as f64) * 100.0;
    let _ = std::fs::remove_dir_all(&hfs_repo);

    let speedup = lfs_result
        .as_ref()
        .map(|lfs| lfs.median.as_secs_f64() / hfs_result.median.as_secs_f64());

    ScenarioResult {
        hfs_time: hfs_result.median,
        lfs_time: lfs_result.map(|r| r.median),
        speedup,
        extra: format!(
            "storage: hfs={} vs lfs={} ({:.1}% saved)",
            utils::human_bytes(hfs_stored),
            utils::human_bytes(lfs_stored),
            savings
        ),
    }
}

// ---------------------------------------------------------------------------
// Scenario: multi-file batch (filter-process vs fork-per-file)
// ---------------------------------------------------------------------------

fn scenario_multi_file(root: &Path, has_lfs: bool) -> ScenarioResult {
    let files: Vec<Vec<u8>> = (0..MULTI_FILE_COUNT as u64)
        .map(|i| utils::generate_data(MULTI_FILE_SIZE, utils::SEED + i))
        .collect();

    let write_all = |repo: &Path| {
        for (i, data) in files.iter().enumerate() {
            write_bin_file(repo, &format!("file_{:04}.bin", i), data);
        }
    };

    let hfs_result = time_iterations(ITERATIONS, || {
        let repo = setup_hfs_repo(root, "hfs-multi");
        write_all(&repo);
        git_add_commit(&repo, "add 100 files");
        let _ = std::fs::remove_dir_all(&repo);
    });

    let lfs_result = if has_lfs {
        Some(time_iterations(ITERATIONS, || {
            let repo = setup_lfs_repo(root, "lfs-multi");
            write_all(&repo);
            git_add_commit(&repo, "add 100 files");
            let _ = std::fs::remove_dir_all(&repo);
        }))
    } else {
        None
    };

    let speedup = lfs_result
        .as_ref()
        .map(|lfs| lfs.median.as_secs_f64() / hfs_result.median.as_secs_f64());

    ScenarioResult {
        hfs_time: hfs_result.median,
        lfs_time: lfs_result.map(|r| r.median),
        speedup,
        extra: format!(
            "{} x {} each",
            MULTI_FILE_COUNT,
            utils::human_bytes(MULTI_FILE_SIZE as u64)
        ),
    }
}

// ---------------------------------------------------------------------------
// Report printer
// ---------------------------------------------------------------------------

struct Row {
    scenario: String,
    size: String,
    hfs: String,
    lfs: String,
    speedup: String,
    extra: String,
}

fn print_report(rows: &[Row]) {
    println!();
    println!("================================================================");
    println!("  HFS vs git-lfs  --  End-to-End Benchmark Results");
    println!("================================================================");
    println!(
        "{:<22} {:<10} {:>10} {:>10} {:>10}",
        "Scenario", "Size", "hfs", "git-lfs", "Speedup"
    );
    println!("{}", "-".repeat(66));

    for row in rows {
        println!(
            "{:<22} {:<10} {:>10} {:>10} {:>10}",
            row.scenario, row.size, row.hfs, row.lfs, row.speedup
        );
        if !row.extra.is_empty() {
            println!("  {}", row.extra);
        }
    }

    println!("{}", "-".repeat(66));
    println!("  Speedup = git-lfs time / hfs time (higher is better for hfs)");
    println!("  Median of {} iterations per scenario", ITERATIONS);
    println!();
}

fn write_json_results(rows: &[Row]) {
    let results: Vec<_> = rows
        .iter()
        .map(|r| {
            json!({
                "scenario": r.scenario,
                "size": r.size,
                "hfs": r.hfs,
                "lfs": r.lfs,
                "speedup": r.speedup,
                "extra": r.extra,
            })
        })
        .collect();

    let json = serde_json::to_string_pretty(&json!({
        "benchmark": "hfs_vs_git_lfs",
        "iterations": ITERATIONS,
        "results": results,
    }))
    .unwrap();

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bench_results.json");
    std::fs::write(&path, &json).unwrap();
    println!("Results written to {}", path.display());
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() {
    let hfs_bin = hfs_binary();
    if !hfs_bin.exists() {
        eprintln!("ERROR: hfs binary not found at {}", hfs_bin.display());
        eprintln!("Run `cargo build --release` first.");
        std::process::exit(1);
    }

    let has_lfs = utils::command_exists("git-lfs");
    if !has_lfs {
        eprintln!("WARNING: git-lfs not found on PATH -- LFS benchmarks will be skipped.");
        eprintln!("         Install git-lfs to see full comparison results.");
        eprintln!();
    }

    let root = tempfile::tempdir().unwrap();
    let mut rows = Vec::new();

    // -- add+commit for each file size --
    for &(size, label) in FILE_SIZES {
        let data = utils::generate_data(size, utils::SEED);
        let r = scenario_add_commit(root.path(), &data, has_lfs);
        rows.push(Row {
            scenario: "add+commit".into(),
            size: label.into(),
            hfs: fmt_duration(r.hfs_time),
            lfs: r.lfs_time.map(fmt_duration).unwrap_or_else(|| "-".into()),
            speedup: r
                .speedup
                .map(|s| format!("{:.1}x", s))
                .unwrap_or_else(|| "-".into()),
            extra: r.extra,
        });
    }

    // -- version_edit for each file size --
    for &(size, label) in FILE_SIZES {
        let data = utils::generate_data(size, utils::SEED);
        let r = scenario_version_edit(root.path(), &data, has_lfs);
        rows.push(Row {
            scenario: "version_edit (1%)".into(),
            size: label.into(),
            hfs: fmt_duration(r.hfs_time),
            lfs: r.lfs_time.map(fmt_duration).unwrap_or_else(|| "-".into()),
            speedup: r
                .speedup
                .map(|s| format!("{:.1}x", s))
                .unwrap_or_else(|| "-".into()),
            extra: r.extra,
        });
    }

    // -- multi_file batch --
    {
        let r = scenario_multi_file(root.path(), has_lfs);
        rows.push(Row {
            scenario: "multi_file (100x)".into(),
            size: "1 MB".into(),
            hfs: fmt_duration(r.hfs_time),
            lfs: r.lfs_time.map(fmt_duration).unwrap_or_else(|| "-".into()),
            speedup: r
                .speedup
                .map(|s| format!("{:.1}x", s))
                .unwrap_or_else(|| "-".into()),
            extra: r.extra,
        });
    }

    print_report(&rows);
    write_json_results(&rows);
}
