use std::path::{Path, PathBuf};

use crate::errors::{DotenvzError, Result};
use super::model::DotenvzConfig;

/// The canonical config filename.
pub const CONFIG_FILENAME: &str = ".dotenvz.toml";

/// Walk up the directory tree from `start` to find the nearest `.dotenvz.toml`.
///
/// Returns `None` if no config file is found before reaching the filesystem root.
pub fn find_config_file(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        let candidate = current.join(CONFIG_FILENAME);
        if candidate.exists() {
            return Some(candidate);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Load and parse a `.dotenvz.toml` at the given path.
pub fn load_config(path: &Path) -> Result<DotenvzConfig> {
    let raw = std::fs::read_to_string(path)?;
    toml::from_str(&raw).map_err(|e| DotenvzError::ConfigParse(e.to_string()))
}

/// Serialize a config to TOML and write it to the given path.
pub fn write_config(path: &Path, config: &DotenvzConfig) -> Result<()> {
    let raw =
        toml::to_string_pretty(config).map_err(|e| DotenvzError::ConfigParse(e.to_string()))?;
    std::fs::write(path, raw)?;
    Ok(())
}
