mod utils;

use criterion::{BenchmarkId, Criterion, criterion_group};

use hfs::cas;

// ---------------------------------------------------------------------------
// Dedup efficiency: measure storage amplification across file versions
//
// For each edit percentage, we ingest a base file then a modified version
// and compare total storage used vs the naive LFS approach (full copy per
// version). The Criterion measurement captures ingest wall-time; custom
// counters report storage savings.
// ---------------------------------------------------------------------------

const BASE_SIZE: usize = utils::SIZE_100MB;
const EDIT_PCTS: &[(f64, &str)] = &[
    (0.0001, "0.01%"),
    (0.001, "0.1%"),
    (0.01, "1%"),
    (0.1, "10%"),
];

fn bench_dedup(c: &mut Criterion) {
    let mut group = c.benchmark_group("dedup_ingest_v2");

    let base_data = utils::generate_data(BASE_SIZE, utils::SEED);

    for &(edit_pct, label) in EDIT_PCTS {
        let mut v2_data = base_data.clone();
        utils::apply_edit(&mut v2_data, edit_pct, utils::SEED + 1);

        group.bench_with_input(
            BenchmarkId::new("ingest_edited_version", label),
            &v2_data,
            |b, v2| {
                b.iter_with_setup(
                    || {
                        let dir = tempfile::tempdir().unwrap();
                        let store = utils::temp_store(dir.path());
                        cas::ingest_bytes(&store, &base_data).unwrap();
                        (dir, store)
                    },
                    |(_dir, store)| {
                        cas::ingest_bytes(&store, v2).unwrap();
                    },
                );
            },
        );
    }

    group.finish();
}

/// Print a comprehensive storage efficiency report (runs once, not via Criterion).
fn dedup_storage_report() {
    println!();
    println!("==========================================================");
    println!("  HFS Dedup Efficiency vs Git LFS (100 MB base file)");
    println!("==========================================================");
    println!(
        "{:<10} {:>14} {:>14} {:>10} {:>10}",
        "Edit %", "hfs stored", "LFS stored", "Savings", "hfs chunks"
    );
    println!("{}", "-".repeat(62));

    let base_data = utils::generate_data(BASE_SIZE, utils::SEED);

    for &(edit_pct, label) in EDIT_PCTS {
        let dir = tempfile::tempdir().unwrap();
        let store = utils::temp_store(dir.path());

        // Ingest base version
        let (ptr_v1, _) = cas::ingest_bytes(&store, &base_data).unwrap();

        // Ingest edited version
        let mut v2_data = base_data.clone();
        utils::apply_edit(&mut v2_data, edit_pct, utils::SEED + 1);
        let (ptr_v2, _) = cas::ingest_bytes(&store, &v2_data).unwrap();

        let hfs_total =
            utils::store_objects_size(&store) + utils::store_manifests_size(&store);
        let lfs_total = (BASE_SIZE as u64) * 2; // LFS stores full blob for each version

        let savings_pct = if lfs_total > 0 {
            (1.0 - hfs_total as f64 / lfs_total as f64) * 100.0
        } else {
            0.0
        };

        let total_chunks = ptr_v1.chunk_count + ptr_v2.chunk_count;
        let unique_objects = store.list_objects().unwrap().len();

        println!(
            "{:<10} {:>14} {:>14} {:>9.1}% {:>5}/{:<4}",
            label,
            utils::human_bytes(hfs_total),
            utils::human_bytes(lfs_total),
            savings_pct,
            unique_objects,
            total_chunks,
        );
    }

    println!("{}", "-".repeat(62));
    println!("  hfs chunks = unique objects / total chunk refs across v1+v2");
    println!("  LFS stored = 2 * file_size (one full blob per version)");
    println!();
}

// ---------------------------------------------------------------------------
// Criterion configuration & main
// ---------------------------------------------------------------------------

fn config() -> Criterion {
    Criterion::default()
        .sample_size(10)
        .warm_up_time(std::time::Duration::from_secs(1))
        .measurement_time(std::time::Duration::from_secs(5))
}

criterion_group! {
    name = benches;
    config = config();
    targets = bench_dedup
}

fn main() {
    // Print the storage report first (non-Criterion, runs once)
    dedup_storage_report();

    // Then run Criterion benchmarks on ingest speed for v2
    let mut criterion = config();
    bench_dedup(&mut criterion);
    // Finalize Criterion (writes HTML reports, etc.)
}
