use std::path::{Path, PathBuf};

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
        Self::resolve_from(&cwd, profile_override)
    }

    /// Resolve from an explicit starting path (useful for testing).
    ///
    /// Walks up from `start` to find the nearest `.dotenvz.toml`, then loads
    /// and validates the config.
    pub fn resolve_from(start: &Path, profile_override: Option<&str>) -> Result<Self> {
        let config_path = find_config_file(start).ok_or(DotenvzError::ConfigNotFound)?;

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
    pub fn project_dir(&self) -> &Path {
        self.config_path
            .parent()
            .expect("config path must have a parent directory")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{write_config, DotenvzConfig, CONFIG_FILENAME};
    use std::fs;
    use tempfile::TempDir;

    fn make_config(dir: &Path, project: &str) {
        let cfg = DotenvzConfig::scaffold(project);
        write_config(&dir.join(CONFIG_FILENAME), &cfg).unwrap();
    }

    #[test]
    fn resolves_config_from_start_dir() {
        let dir = TempDir::new().unwrap();
        make_config(dir.path(), "proj");
        let ctx = ProjectContext::resolve_from(dir.path(), None).unwrap();
        assert_eq!(ctx.config.project, "proj");
        assert_eq!(ctx.profile, "dev");
    }

    #[test]
    fn resolves_config_by_walking_up() {
        let dir = TempDir::new().unwrap();
        make_config(dir.path(), "proj");
        let deep = dir.path().join("nested/dir");
        fs::create_dir_all(&deep).unwrap();
        let ctx = ProjectContext::resolve_from(&deep, None).unwrap();
        assert_eq!(ctx.config.project, "proj");
    }

    #[test]
    fn profile_override_takes_precedence() {
        let dir = TempDir::new().unwrap();
        make_config(dir.path(), "proj");
        let ctx = ProjectContext::resolve_from(dir.path(), Some("production")).unwrap();
        assert_eq!(ctx.profile, "production");
    }

    #[test]
    fn returns_error_when_no_config_found() {
        let dir = TempDir::new().unwrap();
        let err = ProjectContext::resolve_from(dir.path(), None).unwrap_err();
        assert!(matches!(err, DotenvzError::ConfigNotFound));
    }

    #[test]
    fn project_dir_is_config_parent() {
        let dir = TempDir::new().unwrap();
        make_config(dir.path(), "proj");
        let ctx = ProjectContext::resolve_from(dir.path(), None).unwrap();
        assert_eq!(ctx.project_dir(), dir.path());
    }
}
