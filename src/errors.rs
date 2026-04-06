use thiserror::Error;

/// Central error type for dotenvz.
#[derive(Debug, Error)]
pub enum DotenvzError {
    #[error("Config file not found. Run `dotenvz init` to create one.")]
    ConfigNotFound,

    #[error("Failed to parse config: {0}")]
    ConfigParse(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Unknown command or alias: `{0}`")]
    UnknownCommand(String),

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Key not found: `{key}` in profile `{profile}`")]
    KeyNotFound { key: String, profile: String },

    #[error("Process execution error: {0}")]
    ProcessExec(String),

    #[error("Import error: {0}")]
    Import(String),

    #[error("this platform is not yet supported by dotenvz (supported: macOS, Linux, Windows)")]
    UnsupportedPlatform,
}

pub type Result<T> = std::result::Result<T, DotenvzError>;
