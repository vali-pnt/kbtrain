use std::{fs, path::PathBuf};

use anyhow::Result;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub layout: String,
    pub theme: String,
    /// Number of words per drill.
    pub drill_length: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            layout: "qwerty".into(),
            theme: "default".into(),
            drill_length: 25,
        }
    }
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        ProjectDirs::from("", "", "kbtrain").map(|d| d.config_dir().join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        match Self::path() {
            Some(p) if p.exists() => Ok(toml::from_str(&fs::read_to_string(p)?)?),
            _ => Ok(Self::default()),
        }
    }
}
