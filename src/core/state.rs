use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use crate::defs;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct RuntimeState {
    pub timestamp: u64,
    pub pid: u32,
    pub storage_mode: String,
    pub mount_point: PathBuf,
    pub overlay_modules: Vec<String>,
    pub magic_modules: Vec<String>,
    pub nuke_active: bool,
    #[serde(default)]
    pub active_mounts: Vec<String>,
}

impl RuntimeState {
    pub fn new(
        storage_mode: String, 
        mount_point: PathBuf, 
        overlay_modules: Vec<String>, 
        magic_modules: Vec<String>,
        nuke_active: bool,
        active_mounts: Vec<String>,
    ) -> Self {
        let start = SystemTime::now();
        let timestamp = start.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let pid = std::process::id();

        Self {
            timestamp,
            pid,
            storage_mode,
            mount_point,
            overlay_modules,
            magic_modules,
            nuke_active,
            active_mounts,
        }
    }

    pub fn save(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(defs::STATE_FILE, json)?;
        Ok(())
    }

    pub fn load() -> Result<Self> {
        if !std::path::Path::new(defs::STATE_FILE).exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(defs::STATE_FILE)?;
        let state = serde_json::from_str(&content)?;
        Ok(state)
    }
}
