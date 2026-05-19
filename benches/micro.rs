mod utils;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

use sha2::{Digest, Sha256};
use std::io::Write;

use hfs::cas;

// ---------------------------------------------------------------------------
// Data sizes to benchmark
// ---------------------------------------------------------------------------

const SIZES: &[(usize, &str)] = &[
    (utils::SIZE_1MB, "1 MB"),
    (utils::SIZE_10MB, "10 MB"),
    (utils::SIZE_100MB, "100 MB"),
];

// ---------------------------------------------------------------------------
// Hash: BLAKE3 (hfs) vs SHA-256 (git-lfs)
// ---------------------------------------------------------------------------

fn bench_hash(c: &mut Criterion) {
    let mut group = c.benchmark_group("hash");

    for &(size, label) in SIZES {
        let data = utils::generate_data(size, utils::SEED);
        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(BenchmarkId::new("blake3", label), &data, |b, data| {
            b.iter(|| blake3::hash(data));
        });

        group.bench_with_input(BenchmarkId::new("sha256", label), &data, |b, data| {
            b.iter(|| Sha256::digest(data));
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Compression: zstd (hfs) vs gzip (git-lfs)
// ---------------------------------------------------------------------------

fn bench_compress(c: &mut Criterion) {
    let mut group = c.benchmark_group("compress");

    for &(size, label) in SIZES {
        let data = utils::generate_data(size, utils::SEED);
        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(
            BenchmarkId::new("zstd_compress", label),
            &data,
            |b, data| {
                b.iter(|| zstd::encode_all(&data[..], 3).unwrap());
            },
        );

        group.bench_with_input(
            BenchmarkId::new("gzip_compress", label),
            &data,
            |b, data| {
                b.iter(|| {
                    let mut encoder =
                        flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
                    encoder.write_all(data).unwrap();
                    encoder.finish().unwrap()
                });
            },
        );
    }

    group.finish();
}

fn bench_decompress(c: &mut Criterion) {
    let mut group = c.benchmark_group("decompress");

    for &(size, label) in SIZES {
        let data = utils::generate_data(size, utils::SEED);
        let zstd_compressed = zstd::encode_all(&data[..], 3).unwrap();
        let gzip_compressed = {
            let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            enc.write_all(&data).unwrap();
            enc.finish().unwrap()
        };

        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(
            BenchmarkId::new("zstd_decompress", label),
            &zstd_compressed,
            |b, compressed| {
                b.iter(|| {
                    let cursor = std::io::Cursor::new(compressed);
                    let mut decoder = zstd::Decoder::new(cursor).unwrap();
                    let mut out = Vec::with_capacity(size);
                    std::io::Read::read_to_end(&mut decoder, &mut out).unwrap();
                    out
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("gzip_decompress", label),
            &gzip_compressed,
            |b, compressed| {
                b.iter(|| {
                    let cursor = std::io::Cursor::new(compressed);
                    let mut decoder = flate2::read::GzDecoder::new(cursor);
                    let mut out = Vec::with_capacity(size);
                    std::io::Read::read_to_end(&mut decoder, &mut out).unwrap();
                    out
                });
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Ingest pipeline: full clean path (chunk + hash + compress + store)
// ---------------------------------------------------------------------------

fn bench_ingest(c: &mut Criterion) {
    let mut group = c.benchmark_group("ingest");

    for &(size, label) in SIZES {
        let data = utils::generate_data(size, utils::SEED);
        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(BenchmarkId::new("hfs_ingest", label), &data, |b, data| {
            let dir = tempfile::tempdir().unwrap();
            let store = utils::temp_store(dir.path());
            b.iter(|| {
                cas::ingest_bytes(&store, data).unwrap();
            });
        });
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Materialize pipeline: full smudge path (read + decompress + reassemble)
// ---------------------------------------------------------------------------

fn bench_materialize(c: &mut Criterion) {
    let mut group = c.benchmark_group("materialize");

    for &(size, label) in SIZES {
        let data = utils::generate_data(size, utils::SEED);
        let dir = tempfile::tempdir().unwrap();
        let store = utils::temp_store(dir.path());
        let (pointer, _) = cas::ingest_bytes(&store, &data).unwrap();

        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(
            BenchmarkId::new("hfs_materialize", label),
            &pointer,
            |b, pointer| {
                b.iter(|| {
                    cas::materialize(&store, pointer).unwrap();
                });
            },
        );
    }

    group.finish();
}

// ---------------------------------------------------------------------------
// Criterion configuration & main
// ---------------------------------------------------------------------------

fn config() -> Criterion {
    Criterion::default()
        .sample_size(10)
        .warm_up_time(std::time::Duration::from_secs(2))
        .measurement_time(std::time::Duration::from_secs(5))
}

criterion_group! {
    name = benches;
    config = config();
    targets = bench_hash, bench_compress, bench_decompress, bench_ingest, bench_materialize
}

criterion_main!(benches);
