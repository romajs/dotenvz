use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Top-level config model for `.dotenvz.toml`.
///
/// Placed in the project root; dotenvz walks up the directory tree to find it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DotenvzConfig {
    /// Project identifier used as the Keychain namespace.
    /// Must be unique across projects on the same machine.
    pub project: String,

    /// Secret provider backend. Currently only `"macos-keychain"` is supported.
    #[serde(default = "default_provider")]
    pub provider: String,

    /// Default profile when `--profile` is not specified.
    #[serde(default = "default_profile")]
    pub default_profile: String,

    /// Path to a schema file listing expected/required env keys.
    /// Used for future validation and documentation purposes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_file: Option<String>,

    /// Path to the `.env` file used for `dotenvz import`.
    /// This file is never used as a runtime source of truth.
    #[serde(default = "default_import_file")]
    pub import_file: String,

    /// Named command aliases. `dotenvz <name>` resolves to the mapped command
    /// string, fetches secrets, and runs the command with env injected.
    ///
    /// Example:
    /// ```toml
    /// [commands]
    /// dev   = "next dev"
    /// build = "next build"
    /// ```
    #[serde(default)]
    pub commands: HashMap<String, String>,
}

fn default_provider() -> String {
    "macos-keychain".to_string()
}

fn default_profile() -> String {
    "dev".to_string()
}

fn default_import_file() -> String {
    ".env".to_string()
}

impl DotenvzConfig {
    /// Produce a minimal scaffold config for a new project.
    ///
    /// Used by `dotenvz init` to generate the initial `.dotenvz.toml`.
    pub fn scaffold(project_name: impl Into<String>) -> Self {
        let mut commands = HashMap::new();
        commands.insert(
            "dev".to_string(),
            "echo 'replace with your dev command'".to_string(),
        );
        commands.insert(
            "build".to_string(),
            "echo 'replace with your build command'".to_string(),
        );

        Self {
            project: project_name.into(),
            provider: default_provider(),
            default_profile: default_profile(),
            schema_file: Some(".env.example".to_string()),
            import_file: default_import_file(),
            commands,
        }
    }
}
