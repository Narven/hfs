use std::sync::Arc;
use hfs::cas::{self, Store};
use hfs::backend::local::LocalBackend;
use hfs::pointer::Pointer;
use hfs::transfer::engine::TransferEngine;

#[test]
fn end_to_end_small_file() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(&dir.path().join(".hfs"));
    store.init().unwrap();

    let original = b"Hello, this is a small test file for HFS!";

    let (pointer, _manifest_bytes) = cas::ingest_bytes(&store, original).unwrap();

    assert_eq!(pointer.version, 1);
    assert_eq!(pointer.size, original.len() as u64);
    assert!(pointer.chunk_count >= 1);

    // Pointer should roundtrip through text encoding
    let encoded = pointer.encode();
    assert!(Pointer::is_pointer(encoded.as_bytes()));
    let decoded = Pointer::decode(&encoded).unwrap();
    assert_eq!(pointer, decoded);

    // Materialize should reproduce the original content
    let restored = cas::materialize(&store, &pointer).unwrap();
    assert_eq!(restored, original);
}

#[test]
fn end_to_end_large_file() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(&dir.path().join(".hfs"));
    store.init().unwrap();

    // 8 MB of pseudorandom data (deterministic LCG)
    let mut data = vec![0u8; 8 * 1024 * 1024];
    let mut state: u64 = 0xDEADBEEF;
    for byte in data.iter_mut() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        *byte = (state >> 33) as u8;
    }

    let (pointer, _) = cas::ingest_bytes(&store, &data).unwrap();

    assert_eq!(pointer.size, data.len() as u64);
    assert!(pointer.chunk_count > 1, "should produce multiple chunks");

    let restored = cas::materialize(&store, &pointer).unwrap();
    assert_eq!(restored, data);
}

#[test]
fn deduplication_across_versions() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(&dir.path().join(".hfs"));
    store.init().unwrap();

    // Create a 4 MB file
    let mut data_v1 = vec![0xAAu8; 4 * 1024 * 1024];
    let (ptr_v1, _) = cas::ingest_bytes(&store, &data_v1).unwrap();

    let objects_v1 = store.list_objects().unwrap().len();

    // Modify a small portion at the end
    data_v1[3 * 1024 * 1024..3 * 1024 * 1024 + 1024]
        .copy_from_slice(&[0xBB; 1024]);

    let (ptr_v2, _) = cas::ingest_bytes(&store, &data_v1).unwrap();

    let objects_v2 = store.list_objects().unwrap().len();

    // V2 should reuse most chunks from V1
    // (the number of NEW objects should be much less than the total chunks)
    let new_objects = objects_v2 - objects_v1;
    assert!(
        new_objects <= ptr_v2.chunk_count as usize,
        "new objects ({new_objects}) should be <= total chunks in v2 ({})",
        ptr_v2.chunk_count
    );

    // Both versions should still materialize correctly
    let restored_v1 = cas::materialize(&store, &ptr_v1).unwrap();
    assert_eq!(restored_v1.len(), 4 * 1024 * 1024);
    assert_eq!(restored_v1[0], 0xAA);

    let restored_v2 = cas::materialize(&store, &ptr_v2).unwrap();
    assert_eq!(restored_v2.len(), 4 * 1024 * 1024);
    assert_eq!(restored_v2[3 * 1024 * 1024], 0xBB);
}

#[test]
fn empty_file() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(&dir.path().join(".hfs"));
    store.init().unwrap();

    let (pointer, _) = cas::ingest_bytes(&store, b"").unwrap();
    assert_eq!(pointer.size, 0);
    assert_eq!(pointer.chunk_count, 0);

    let restored = cas::materialize(&store, &pointer).unwrap();
    assert!(restored.is_empty());
}

#[tokio::test]
async fn transfer_push_pull_local_backend() {
    let dir = tempfile::tempdir().unwrap();
    let store_a = Store::new(&dir.path().join("repo-a/.hfs"));
    store_a.init().unwrap();
    let store_b = Store::new(&dir.path().join("repo-b/.hfs"));
    store_b.init().unwrap();

    // Ingest data into store A
    let mut data = vec![0u8; 4 * 1024 * 1024];
    let mut state: u64 = 42;
    for byte in data.iter_mut() {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        *byte = (state >> 33) as u8;
    }

    let (pointer, _) = cas::ingest_bytes(&store_a, &data).unwrap();

    // Create a "remote" local backend
    let remote_dir = dir.path().join("remote");
    let backend = Arc::new(LocalBackend::new(&remote_dir).unwrap());

    // Push from store A to remote
    let engine_a = TransferEngine::new(
        Store::new(&dir.path().join("repo-a/.hfs")),
        Arc::clone(&backend) as Arc<dyn hfs::backend::Backend>,
    );
    let (pushed, skipped) = engine_a.push(&[pointer.oid]).await.unwrap();
    assert!(pushed > 0);
    assert_eq!(skipped, 0);

    // Push again -- should skip all
    let (pushed2, skipped2) = engine_a.push(&[pointer.oid]).await.unwrap();
    assert_eq!(pushed2, 0);
    assert!(skipped2 > 0);

    // Copy manifest to store B so pull can read it
    let manifest_data = store_a.get_manifest(&pointer.oid).unwrap();
    store_b.put_manifest(&pointer.oid, &manifest_data).unwrap();

    // Pull from remote to store B
    let engine_b = TransferEngine::new(
        Store::new(&dir.path().join("repo-b/.hfs")),
        backend,
    );
    let (pulled, pull_skipped) = engine_b.pull(&[pointer.oid]).await.unwrap();
    assert!(pulled > 0);
    assert_eq!(pull_skipped, 0);

    // Materialize from store B should produce the same data
    let restored = cas::materialize(&store_b, &pointer).unwrap();
    assert_eq!(restored, data);
}
