# Architecture

> Internal design reference for HFS contributors.
> For usage, see [README.md](../README.md).

---

## The problem with Git LFS

Git LFS replaces large files with pointer files and stores the actual content on a remote server. Simple idea, terrible execution:

- **No deduplication.** Every version is a full blob. Edit one byte in 500 MB and you store another 500 MB.
- **Slow hashing.** SHA-256 with no parallelism.
- **Slow compression.** gzip.
- **Fork per file.** Every `git add` spawns a new filter process for each blob.
- **Sequential transfers.** One HTTP request at a time.

HFS replaces every one of these layers.

---

## Data model

### Chunks

Files are split using **FastCDC** (content-defined chunking) with a target range of 256 KB -- 4 MB. Content-defined boundaries mean that inserting or deleting bytes in one region does not shift the chunk boundaries elsewhere in the file. A one-byte edit typically invalidates only 1--2 chunks.

Each chunk is:
1. Hashed with **BLAKE3** (SIMD-accelerated, tree-hashable).
2. Compressed with **zstd** (level 3 -- fast, good ratio).
3. Written to the content-addressable store, keyed by its hash.

Duplicate chunks (same content across files or versions) are stored exactly once.

### Manifests

A manifest is an ordered list of chunk references that describes how to reassemble one file. Serialized as **MessagePack** for compact binary encoding.

```rust
struct Manifest {
    version: u8,
    file_size: u64,
    chunk_size_avg: u32,
    chunks: Vec<ChunkRef>,
}

struct ChunkRef {
    hash: [u8; 32],    // BLAKE3
    offset: u64,        // byte offset in original file
    size: u32,          // compressed size on disk
    original_size: u32, // decompressed size
}
```

### Pointers

What Git actually stores. Plain text, under 256 bytes:

```
hfs v1
oid blake3:<manifest-hash>
size <file-size>
chunks <count>
```

---

## Pipelines

### Clean (file -> pointer)

```
file bytes
  -> FastCDC split
  -> per-chunk: BLAKE3 hash, zstd compress, write to .hfs/objects/
  -> build manifest (ordered chunk list)
  -> serialize manifest, write to .hfs/manifests/
  -> emit pointer (manifest hash + metadata)
```

### Smudge (pointer -> file)

```
pointer text
  -> parse manifest hash
  -> read manifest from .hfs/manifests/
  -> for each chunk: read from .hfs/objects/, zstd decompress
  -> concatenate in order
  -> emit original file bytes
```

Both pipelines run inside a **long-running Git filter process** (Git's `process` protocol over pkt-line). One process handles every blob in a single `git add` or `git checkout` -- no per-file fork overhead.

---

## On-disk layout

```
.hfs/
  config.toml              Configuration (chunk sizes, remote backend)
  objects/
    <2-hex-prefix>/
      <remaining-hex>      Compressed chunk data
  manifests/
    <2-hex-prefix>/
      <remaining-hex>      MessagePack-encoded manifest
  tmp/                     Staging area for atomic writes
```

All writes go to `tmp/` first and are atomically renamed into place. If the destination already exists, the write is a no-op (content-addressable -- same hash means same content).

---

## Remote transfer

The `Backend` trait abstracts remote storage:

```rust
#[async_trait]
trait Backend: Send + Sync {
    async fn push_chunk(&self, hash: &[u8; 32], data: &[u8]) -> Result<()>;
    async fn pull_chunk(&self, hash: &[u8; 32]) -> Result<Vec<u8>>;
    async fn has_chunk(&self, hash: &[u8; 32]) -> Result<bool>;
    async fn list_chunks(&self) -> Result<Vec<[u8; 32]>>;
}
```

Implementations: **S3-compatible** (AWS, MinIO, R2, GCS) and **local filesystem** (testing, NFS shares).

The transfer engine diffs the local and remote chunk sets, then pushes or pulls only missing chunks with **32 concurrent tokio tasks** gated by a semaphore.

---

## Module map

```
src/
  cas/
    chunk.rs         FastCDC content-defined chunking
    hash.rs          BLAKE3 (SIMD, parallel tree hashing)
    compress.rs      zstd compress/decompress
    store.rs         Local CAS: atomic put/get, 2-char prefix dirs
  manifest.rs        Manifest serialize/deserialize (MessagePack)
  pointer.rs         Pointer format parse/emit
  filter/
    pktline.rs       Git pkt-line reader/writer
    process.rs       Long-running process filter (clean + smudge)
  backend/
    local.rs         Local filesystem backend
    s3.rs            S3-compatible backend (aws-sdk-s3)
  transfer/
    engine.rs        Parallel push/pull with semaphore concurrency
  cli/               One module per command
  config.rs          TOML config loader
```

---

## Dependencies

| Crate | Role |
|---|---|
| `blake3` | Hashing (SIMD, rayon parallel) |
| `fastcdc` v3 | Content-defined chunking |
| `zstd` | Compression |
| `tokio` | Async runtime for transfers |
| `clap` | CLI |
| `rmp-serde` | MessagePack for manifests |
| `aws-sdk-s3` | S3 backend |
| `anyhow` / `thiserror` | Error handling |
| `tracing` | Structured logging |
