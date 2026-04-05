use std::path::PathBuf;

use crate::config::{find_config_file, load_config, DotenvzConfig};
use crate::errors::{DotenvzError, Result};

/// Fully resolved context for the current project invocation.
///
/// Constructed once at startup and passed into each command handler.
#[derive(Debug)]
pub struct ProjectContext {
    /// Absolute path to the `.dotenvz.toml` file.
    pub config_path: PathBuf,
    /// Parsed configuration.
    pub config: DotenvzConfig,
    /// Active profile — from `--profile` flag or `default_profile` in config.
    pub profile: String,
}

impl ProjectContext {
    /// Resolve the project context by walking up from the current directory.
    ///
    /// Returns an error if no `.dotenvz.toml` is found in the directory tree.
    pub fn resolve(profile_override: Option<&str>) -> Result<Self> {
        let cwd = std::env::current_dir()?;
        let config_path =
            find_config_file(&cwd).ok_or(DotenvzError::ConfigNotFound)?;

        let config = load_config(&config_path)?;

        let profile = profile_override
            .map(str::to_string)
            .unwrap_or_else(|| config.default_profile.clone());

        Ok(Self {
            config_path,
            config,
            profile,
        })
    }

    /// The directory containing `.dotenvz.toml` (the project root).
    pub fn project_dir(&self) -> &std::path::Path {
        self.config_path
            .parent()
            .expect("config path must have a parent directory")
    }
}
