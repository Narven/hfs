use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub store: StoreConfig,
    #[serde(default)]
    pub remote: Option<RemoteConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreConfig {
    #[serde(default = "default_min_chunk")]
    pub min_chunk_size: u32,
    #[serde(default = "default_avg_chunk")]
    pub avg_chunk_size: u32,
    #[serde(default = "default_max_chunk")]
    pub max_chunk_size: u32,
    #[serde(default = "default_compression_level")]
    pub compression_level: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteConfig {
    pub backend: String,
    pub bucket: Option<String>,
    pub region: Option<String>,
    pub endpoint: Option<String>,
    pub prefix: Option<String>,
}

fn default_min_chunk() -> u32 { 256 * 1024 }
fn default_avg_chunk() -> u32 { 1024 * 1024 }
fn default_max_chunk() -> u32 { 4 * 1024 * 1024 }
fn default_compression_level() -> i32 { 3 }

impl Default for Config {
    fn default() -> Self {
        Self {
            store: StoreConfig::default(),
            remote: None,
        }
    }
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            min_chunk_size: default_min_chunk(),
            avg_chunk_size: default_avg_chunk(),
            max_chunk_size: default_max_chunk(),
            compression_level: default_compression_level(),
        }
    }
}

impl Config {
    pub fn load(hfs_dir: &Path) -> Result<Self> {
        let path = hfs_dir.join("config.toml");
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        Ok(toml::from_str(&contents)?)
    }

    pub fn save(&self, hfs_dir: &Path) -> Result<()> {
        let path = hfs_dir.join("config.toml");
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(&path, contents)?;
        Ok(())
    }

    /// Walk up from `start` to find the `.hfs` directory.
    pub fn find_hfs_dir(start: &Path) -> Option<PathBuf> {
        let mut dir = start.to_path_buf();
        loop {
            let candidate = dir.join(".hfs");
            if candidate.is_dir() {
                return Some(candidate);
            }
            if !dir.pop() {
                return None;
            }
        }
    }
}
