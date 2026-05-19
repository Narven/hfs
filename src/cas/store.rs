use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use super::hash::hash_to_hex;

/// Local content-addressable store under `.hfs/`.
pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Initialize the store directory structure.
    pub fn init(&self) -> Result<()> {
        fs::create_dir_all(self.objects_dir())?;
        fs::create_dir_all(self.manifests_dir())?;
        fs::create_dir_all(self.tmp_dir())?;
        Ok(())
    }

    fn objects_dir(&self) -> PathBuf {
        self.root.join("objects")
    }

    fn manifests_dir(&self) -> PathBuf {
        self.root.join("manifests")
    }

    fn tmp_dir(&self) -> PathBuf {
        self.root.join("tmp")
    }

    fn object_path(&self, hash: &[u8; 32]) -> PathBuf {
        let hex = hash_to_hex(hash);
        let (prefix, rest) = hex.split_at(2);
        self.objects_dir().join(prefix).join(rest)
    }

    fn manifest_path(&self, hash: &[u8; 32]) -> PathBuf {
        let hex = hash_to_hex(hash);
        let (prefix, rest) = hex.split_at(2);
        self.manifests_dir().join(prefix).join(rest)
    }

    /// Atomically write data to a content-addressed path.
    fn atomic_write(&self, dest: &Path, data: &[u8]) -> Result<()> {
        if dest.exists() {
            return Ok(());
        }

        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }

        let tmp_path = self.tmp_dir().join(format!(
            "tmp-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));

        fs::create_dir_all(self.tmp_dir())?;
        fs::write(&tmp_path, data).context("writing temp file")?;

        if let Err(e) = fs::rename(&tmp_path, dest) {
            let _ = fs::remove_file(&tmp_path);
            if dest.exists() {
                return Ok(());
            }
            return Err(e).context("renaming temp to final");
        }

        Ok(())
    }

    pub fn put_object(&self, hash: &[u8; 32], data: &[u8]) -> Result<()> {
        self.atomic_write(&self.object_path(hash), data)
    }

    pub fn get_object(&self, hash: &[u8; 32]) -> Result<Vec<u8>> {
        let path = self.object_path(hash);
        fs::read(&path).with_context(|| format!("reading object {}", hash_to_hex(hash)))
    }

    pub fn has_object(&self, hash: &[u8; 32]) -> bool {
        self.object_path(hash).exists()
    }

    pub fn put_manifest(&self, hash: &[u8; 32], data: &[u8]) -> Result<()> {
        self.atomic_write(&self.manifest_path(hash), data)
    }

    pub fn get_manifest(&self, hash: &[u8; 32]) -> Result<Vec<u8>> {
        let path = self.manifest_path(hash);
        fs::read(&path).with_context(|| format!("reading manifest {}", hash_to_hex(hash)))
    }

    pub fn has_manifest(&self, hash: &[u8; 32]) -> bool {
        self.manifest_path(hash).exists()
    }

    /// List all object hashes in the store (for GC).
    pub fn list_objects(&self) -> Result<Vec<[u8; 32]>> {
        self.list_hashes(&self.objects_dir())
    }

    /// List all manifest hashes in the store.
    pub fn list_manifests(&self) -> Result<Vec<[u8; 32]>> {
        self.list_hashes(&self.manifests_dir())
    }

    fn list_hashes(&self, dir: &Path) -> Result<Vec<[u8; 32]>> {
        let mut hashes = Vec::new();
        if !dir.exists() {
            return Ok(hashes);
        }
        for prefix_entry in fs::read_dir(dir)? {
            let prefix_entry = prefix_entry?;
            if !prefix_entry.file_type()?.is_dir() {
                continue;
            }
            let prefix = prefix_entry.file_name().to_string_lossy().to_string();
            for file_entry in fs::read_dir(prefix_entry.path())? {
                let file_entry = file_entry?;
                let rest = file_entry.file_name().to_string_lossy().to_string();
                let hex = format!("{prefix}{rest}");
                if let Ok(hash) = super::hash::hex_to_hash(&hex) {
                    hashes.push(hash);
                }
            }
        }
        Ok(hashes)
    }

    pub fn remove_object(&self, hash: &[u8; 32]) -> Result<()> {
        let path = self.object_path(hash);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }

    pub fn remove_manifest(&self, hash: &[u8; 32]) -> Result<()> {
        let path = self.manifest_path(hash);
        if path.exists() {
            fs::remove_file(&path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(&dir.path().join(".hfs"));
        store.init().unwrap();
        (dir, store)
    }

    #[test]
    fn put_get_object() {
        let (_dir, store) = temp_store();
        let hash = crate::cas::hash::hash_bytes(b"test");
        store.put_object(&hash, b"compressed-data").unwrap();
        let data = store.get_object(&hash).unwrap();
        assert_eq!(data, b"compressed-data");
    }

    #[test]
    fn has_object() {
        let (_dir, store) = temp_store();
        let hash = crate::cas::hash::hash_bytes(b"test");
        assert!(!store.has_object(&hash));
        store.put_object(&hash, b"data").unwrap();
        assert!(store.has_object(&hash));
    }

    #[test]
    fn duplicate_put_is_noop() {
        let (_dir, store) = temp_store();
        let hash = crate::cas::hash::hash_bytes(b"test");
        store.put_object(&hash, b"data1").unwrap();
        store.put_object(&hash, b"data2").unwrap();
        let data = store.get_object(&hash).unwrap();
        assert_eq!(data, b"data1");
    }

    #[test]
    fn list_objects() {
        let (_dir, store) = temp_store();
        let h1 = crate::cas::hash::hash_bytes(b"a");
        let h2 = crate::cas::hash::hash_bytes(b"b");
        store.put_object(&h1, b"data-a").unwrap();
        store.put_object(&h2, b"data-b").unwrap();
        let mut hashes = store.list_objects().unwrap();
        hashes.sort();
        let mut expected = vec![h1, h2];
        expected.sort();
        assert_eq!(hashes, expected);
    }
}
