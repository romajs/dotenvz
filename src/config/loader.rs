use std::path::{Path, PathBuf};

use super::model::DotenvzConfig;
use crate::errors::{DotenvzError, Result};

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
///
/// Validates the parsed config immediately and returns a descriptive error
/// for any invalid field values.
pub fn load_config(path: &Path) -> Result<DotenvzConfig> {
    let raw = std::fs::read_to_string(path)?;
    let config: DotenvzConfig =
        toml::from_str(&raw).map_err(|e| DotenvzError::ConfigParse(e.to_string()))?;
    config.validate()?;
    Ok(config)
}

/// Serialize a config to TOML and write it to the given path.
pub fn write_config(path: &Path, config: &DotenvzConfig) -> Result<()> {
    let raw =
        toml::to_string_pretty(config).map_err(|e| DotenvzError::ConfigParse(e.to_string()))?;
    std::fs::write(path, raw)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_toml(dir: &Path, content: &str) -> PathBuf {
        let p = dir.join(CONFIG_FILENAME);
        fs::write(&p, content).unwrap();
        p
    }

    // --- find_config_file ---

    #[test]
    fn find_config_in_start_dir() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(CONFIG_FILENAME), "").unwrap();
        let found = find_config_file(dir.path());
        assert_eq!(found, Some(dir.path().join(CONFIG_FILENAME)));
    }

    #[test]
    fn find_config_walks_up() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join(CONFIG_FILENAME), "").unwrap();
        let subdir = dir.path().join("a/b/c");
        fs::create_dir_all(&subdir).unwrap();
        let found = find_config_file(&subdir);
        assert_eq!(found, Some(dir.path().join(CONFIG_FILENAME)));
    }

    #[test]
    fn find_config_returns_none_when_absent() {
        let dir = TempDir::new().unwrap();
        assert!(find_config_file(dir.path()).is_none());
    }

    // --- load_config ---

    #[test]
    fn load_valid_config() {
        let dir = TempDir::new().unwrap();
        let raw = r#"
project = "my-app"
provider = "macos-keychain"
default_profile = "dev"
import_file = ".env"

[commands]
dev = "next dev"
"#;
        let path = write_toml(dir.path(), raw);
        let cfg = load_config(&path).unwrap();
        assert_eq!(cfg.project, "my-app");
        assert_eq!(cfg.provider, "macos-keychain");
        assert_eq!(cfg.commands.get("dev").unwrap(), "next dev");
    }

    #[test]
    fn load_config_uses_defaults() {
        let dir = TempDir::new().unwrap();
        let raw = r#"project = "minimal""#;
        let path = write_toml(dir.path(), raw);
        let cfg = load_config(&path).unwrap();
        assert_eq!(cfg.provider, "macos-keychain");
        assert_eq!(cfg.default_profile, "dev");
        assert_eq!(cfg.import_file, ".env");
    }

    #[test]
    fn load_config_rejects_malformed_toml() {
        let dir = TempDir::new().unwrap();
        let path = write_toml(dir.path(), "not valid = [[ toml");
        assert!(load_config(&path).is_err());
    }

    #[test]
    fn load_config_rejects_unknown_provider() {
        let dir = TempDir::new().unwrap();
        let raw = r#"project = "p"\nprovider = "hashicorp-vault""#;
        let path = write_toml(dir.path(), raw);
        let err = load_config(&path).unwrap_err();
        assert!(err.to_string().contains("hashicorp-vault"));
    }

    // --- write_config / round-trip ---

    #[test]
    fn round_trip_write_and_read() {
        let dir = TempDir::new().unwrap();
        let original = DotenvzConfig::scaffold("round-trip-app");
        let path = dir.path().join(CONFIG_FILENAME);
        write_config(&path, &original).unwrap();

        let loaded = load_config(&path).unwrap();
        assert_eq!(loaded.project, original.project);
        assert_eq!(loaded.provider, original.provider);
        assert_eq!(loaded.default_profile, original.default_profile);
        assert_eq!(loaded.commands, original.commands);
    }
}
