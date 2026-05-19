# HFS

**Heavy / Honest File Storage** — a high-performance alternative to Git LFS.

## Why

Git LFS stores whole-file blobs. Change one byte in a 500 MB file and it stores a brand new 500 MB blob. Transfers are sequential. The filter spawns a new process per file. The result: repos with large assets take hours to sync.

HFS fixes every layer.

## How it works

```
Working tree  ──clean──►  FastCDC chunks  ──►  BLAKE3 hash  ──►  zstd compress  ──►  .hfs/objects/
     ▲                                                                                     │
     └──────────smudge◄── reassemble ◄──── decompress ◄──── fetch chunks ◄─────────────────┘
```

Files tracked by HFS are split into content-defined chunks (FastCDC, 256 KB – 4 MB). Each chunk is hashed with BLAKE3, compressed with zstd, and stored in a local content-addressable store. Git sees only a tiny pointer file:

```
hfs v1
oid blake3:ab3f...
size 524288000
chunks 497
```

On checkout, the pointer is resolved back to chunks which are decompressed and reassembled into the original file. All of this happens through Git's long-running process filter protocol -- one persistent process, no per-file fork overhead.

### What this buys you

| Git LFS                      | HFS                                                    |
| ---------------------------- | ------------------------------------------------------ |
| Whole-file blobs -- no dedup | Content-defined chunking -- only changed chunks stored |
| SHA-256                      | BLAKE3 (3-5x faster, SIMD)                             |
| gzip                         | zstd (3-10x faster decompression)                      |
| Process-per-file filter      | Long-running process filter                            |
| Sequential HTTP              | Parallel chunk transfers (tokio, 32 concurrent)        |

## Quick start

```bash
cargo install --path .

cd your-repo
hfs init
hfs track "*.bin" "*.tar.gz"
git add .gitattributes
git add large-file.bin
git commit -m "add large file"
```

That's it. `git add` runs the clean filter (file -> chunks -> pointer). `git checkout` runs the smudge filter (pointer -> chunks -> file).

### Remote storage

Configure S3 in `.hfs/config.toml`:

```toml
[remote]
backend = "s3"
bucket = "my-bucket"
region = "us-east-1"
# endpoint = "http://localhost:9000"  # for MinIO
```

Then:

```bash
hfs push    # upload chunks
hfs pull    # download chunks
hfs clone   # after git clone, fetch all chunks
```

## Commands

| Command                  | Description                                 |
| ------------------------ | ------------------------------------------- |
| `hfs init`               | Initialize store, configure git filter      |
| `hfs track <patterns>`   | Add patterns to `.gitattributes`            |
| `hfs untrack <patterns>` | Remove patterns from `.gitattributes`       |
| `hfs status`             | Store stats, tracked patterns, stored files |
| `hfs ls-files`           | List tracked files with sizes               |
| `hfs push`               | Push chunks to remote                       |
| `hfs pull`               | Pull missing chunks from remote             |
| `hfs clone`              | Fetch all chunks after `git clone`          |
| `hfs gc [--dry-run]`     | Remove orphaned chunks                      |

## Architecture

```
src/
  cas/           Content-addressable store
    chunk.rs       FastCDC chunking
    hash.rs        BLAKE3 hashing
    compress.rs    zstd compression
    store.rs       Local object store (atomic writes, 2-char prefix dirs)
  manifest.rs    File manifests (MessagePack-serialized chunk lists)
  pointer.rs     Pointer file format (parse/emit)
  filter/        Git integration
    pktline.rs     pkt-line protocol
    process.rs     Long-running process filter (clean/smudge)
  backend/       Remote storage
    local.rs       Local filesystem backend
    s3.rs          S3-compatible backend
  transfer/
    engine.rs      Parallel chunk transfer (tokio + semaphore)
  cli/           CLI commands
  config.rs      TOML config
```

### Store layout

```
.hfs/
  config.toml
  objects/       Compressed chunks keyed by BLAKE3 hash
    ab/cdef...
  manifests/     File manifests keyed by hash
    ab/cdef...
  tmp/           Atomic write staging
```

## Building

```bash
cargo build --release
cargo test
```

## Benchmarks

All numbers measured on a single machine (Windows, AMD64). Run `cargo bench` to reproduce.

### Hashing: BLAKE3 vs SHA-256

HFS uses BLAKE3 (SIMD-accelerated, tree-hashing). Git LFS uses SHA-256.

| Size   | BLAKE3 (HFS) | SHA-256 (LFS) | Speedup  |
| ------ | ------------ | ------------- | -------- |
| 1 MB   | 2.30 GiB/s   | 591 MiB/s     | **4.0x** |
| 10 MB  | 2.68 GiB/s   | 621 MiB/s     | **4.4x** |
| 100 MB | 2.95 GiB/s   | 439 MiB/s     | **6.9x** |

### Compression: zstd vs gzip

HFS uses zstd (level 3). Git LFS typically uses gzip.

| Size   | zstd (HFS) | gzip (LFS) | Speedup |
| ------ | ---------- | ---------- | ------- |
| 1 MB   | 307 MiB/s  | 16.6 MiB/s | **18x** |
| 10 MB  | 531 MiB/s  | 20.1 MiB/s | **26x** |
| 100 MB | 388 MiB/s  | 20.9 MiB/s | **19x** |

### Pipeline throughput

Full clean (ingest) and smudge (materialize) paths including chunking, hashing, compression, and I/O.

| Size   | Ingest (clean) | Materialize (smudge) |
| ------ | -------------- | -------------------- |
| 1 MB   | 212 MiB/s      | 289 MiB/s            |
| 10 MB  | 181 MiB/s      | 412 MiB/s            |
| 100 MB | 90 MiB/s       | 437 MiB/s            |

### Dedup efficiency

Edit a 100 MB file and commit both versions. LFS stores two full copies (200 MB). HFS only stores the chunks that actually changed.

| Edit size     | HFS stored | LFS stored | Storage saved |
| ------------- | ---------- | ---------- | ------------- |
| 0.01% (10 KB) | 70.0 MB    | 200 MB     | **65.0%**     |
| 0.1% (100 KB) | 70.1 MB    | 200 MB     | **64.9%**     |
| 1% (1 MB)     | 71.1 MB    | 200 MB     | **64.5%**     |
| 10% (10 MB)   | 81.1 MB    | 200 MB     | **59.5%**     |

### End-to-end: HFS vs git-lfs

Real git workflows, wall-clock time, median of 3 runs.

| Scenario                 | Size      | HFS   | git-lfs | Speedup  |
| ------------------------ | --------- | ----- | ------- | -------- |
| `git add` + `commit`     | 10 MB     | 3.85s | 15.87s  | **4.1x** |
| `git add` + `commit`     | 100 MB    | 5.13s | 12.73s  | **2.5x** |
| Version edit (1% change) | 10 MB     | 3.59s | 29.29s  | **8.2x** |
| Version edit (1% change) | 100 MB    | 9.86s | 19.35s  | **2.0x** |
| 100 files batch          | 1 MB each | 7.27s | 18.53s  | **2.6x** |

Version edit also saved 58-63% storage compared to LFS.

### Running the benchmarks

```bash
cargo build --release

cargo bench --bench micro          # Criterion micro-benchmarks (HTML reports in target/criterion/)
cargo bench --bench dedup          # Dedup storage efficiency report
cargo bench --bench e2e_harness    # End-to-end HFS vs git-lfs (requires git-lfs on PATH)

cargo bench                        # Run all
```
