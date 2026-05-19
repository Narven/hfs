pub mod chunk;
pub mod compress;
pub mod hash;
pub mod store;

use anyhow::Result;
use std::path::Path;

use crate::manifest::{ChunkRef, Manifest};
use crate::pointer::Pointer;

pub use store::Store;

/// Ingest a file: chunk it, hash+compress each chunk, store in CAS, build manifest+pointer.
pub fn ingest_file(store: &Store, path: &Path) -> Result<(Pointer, Vec<u8>)> {
    let file_data = std::fs::read(path)?;
    ingest_bytes(store, &file_data)
}

/// Ingest raw bytes: chunk, hash+compress, store in CAS, return (pointer, manifest_bytes).
pub fn ingest_bytes(store: &Store, data: &[u8]) -> Result<(Pointer, Vec<u8>)> {
    let file_size = data.len() as u64;
    let chunks = chunk::chunk_data(data);

    let mut chunk_refs = Vec::with_capacity(chunks.len());
    let mut offset: u64 = 0;

    for chunk_data in &chunks {
        let hash = hash::hash_bytes(chunk_data);
        let compressed = compress::compress(chunk_data)?;

        store.put_object(&hash, &compressed)?;

        chunk_refs.push(ChunkRef {
            hash,
            offset,
            size: compressed.len() as u32,
            original_size: chunk_data.len() as u32,
        });
        offset += chunk_data.len() as u64;
    }

    let manifest = Manifest {
        version: 1,
        file_size,
        chunk_size_avg: if chunks.is_empty() {
            0
        } else {
            (file_size / chunks.len() as u64) as u32
        },
        chunks: chunk_refs,
    };

    let manifest_bytes = manifest.serialize()?;
    let manifest_hash = hash::hash_bytes(&manifest_bytes);

    store.put_manifest(&manifest_hash, &manifest_bytes)?;

    let pointer = Pointer {
        version: 1,
        oid: manifest_hash,
        size: file_size,
        chunk_count: manifest.chunks.len() as u32,
    };

    Ok((pointer, manifest_bytes))
}

/// Materialize a file from a pointer: read manifest, fetch+decompress chunks, reassemble.
pub fn materialize(store: &Store, pointer: &Pointer) -> Result<Vec<u8>> {
    let manifest_bytes = store.get_manifest(&pointer.oid)?;
    let manifest = Manifest::deserialize(&manifest_bytes)?;

    let mut output = Vec::with_capacity(manifest.file_size as usize);

    for chunk_ref in &manifest.chunks {
        let compressed = store.get_object(&chunk_ref.hash)?;
        let decompressed = compress::decompress(&compressed, chunk_ref.original_size as usize)?;
        output.extend_from_slice(&decompressed);
    }

    Ok(output)
}
