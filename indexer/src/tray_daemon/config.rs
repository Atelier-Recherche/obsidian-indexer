use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrayConfig {
    /// Répertoire racine du vault Obsidian
    pub vault_path: String,
    /// Chemin de la base SQLite relative au vault (comme le CLI `--db`)
    pub db_relative: String,
    /// Pause entre deux passes d’indexation lorsque l’indexeur est actif (secondes)
    #[serde(default = "default_interval")]
    pub interval_seconds: u64,
    #[serde(default)]
    pub strip_code_blocks: bool,
    #[serde(default = "default_max_chunk")]
    pub max_chunk_chars: usize,
    /// Démarrer avec l’indexation automatique activée
    #[serde(default)]
    pub start_enabled: bool,
    /// Démarrer le tray automatiquement à l'ouverture de session Windows
    #[serde(default)]
    pub start_with_windows: bool,
}

fn default_interval() -> u64 {
    120
}

fn default_max_chunk() -> usize {
    8192
}

impl Default for TrayConfig {
    fn default() -> Self {
        Self {
            vault_path: String::new(),
            db_relative: ".obsidian-index/index.sqlite".to_string(),
            interval_seconds: default_interval(),
            strip_code_blocks: false,
            max_chunk_chars: default_max_chunk(),
            start_enabled: false,
            start_with_windows: false,
        }
    }
}

pub fn config_file_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("obsidian-indexer")
        .join("config.json")
}

pub fn control_dir_path() -> PathBuf {
    config_file_path()
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn force_rebuild_flag_path() -> PathBuf {
    control_dir_path().join("force-rebuild.flag")
}

pub fn log_file_path() -> PathBuf {
    control_dir_path().join("tray.log")
}

impl TrayConfig {
    pub fn load_or_default() -> Self {
        let path = config_file_path();
        if path.exists() {
            if let Ok(bytes) = std::fs::read(&path) {
                if let Ok(c) = serde_json::from_slice::<TrayConfig>(&bytes) {
                    return c;
                }
            }
        }
        TrayConfig::default()
    }

    pub fn save(&self) -> Result<()> {
        let path = config_file_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("mkdir {:?}", dir))?;
        }
        let data = serde_json::to_vec_pretty(self).context("serialize config")?;
        std::fs::write(&path, data).with_context(|| format!("write {:?}", path))?;
        Ok(())
    }
}

pub fn resolve_db(vault: &Path, db: &Path) -> PathBuf {
    if db.is_absolute() {
        db.to_path_buf()
    } else {
        vault.join(db)
    }
}
