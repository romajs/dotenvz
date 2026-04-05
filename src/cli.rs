use clap::{Parser, Subcommand};

/// dotenvz — universal CLI for secure env injection via Apple Keychain.
///
/// Secrets are stored in the macOS Keychain, scoped by project and profile.
/// Use `dotenvz init` to scaffold a `.dotenvz.toml` in your project root.
#[derive(Debug, Parser)]
#[command(name = "dotenvz", version, about)]
pub struct Cli {
    /// Override the active profile (defaults to `default_profile` in config).
    #[arg(short, long, global = true)]
    pub profile: Option<String>,

    /// Print what would happen without making any changes or running processes.
    #[arg(long, global = true)]
    pub dry_run: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Scaffold a `.dotenvz.toml` in the current directory.
    Init {
        /// Overwrite an existing `.dotenvz.toml` without prompting.
        #[arg(long)]
        force: bool,
    },

    /// Import variables from a `.env` file into the secret provider.
    Import {
        /// Path to the `.env` file. Defaults to `import_file` in config.
        #[arg(short, long)]
        file: Option<String>,
    },

    /// Store or update a secret in the provider.
    Set {
        /// Environment variable key.
        key: String,
        /// Environment variable value.
        value: String,
    },

    /// Retrieve a single secret from the provider.
    Get {
        /// Environment variable key.
        key: String,
    },

    /// List all secret keys for the current project and profile.
    List,

    /// Remove a secret from the provider.
    Rm {
        /// Environment variable key to remove.
        key: String,
    },

    /// Execute a command with secrets injected as environment variables.
    ///
    /// Usage: dotenvz exec -- <command> [args...]
    Exec {
        /// The command and any arguments to run.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Resolve a named alias from `[commands]` in `.dotenvz.toml`.
    ///
    /// Example: `dotenvz dev` resolves to the value of `commands.dev`.
    #[command(external_subcommand)]
    Alias(Vec<String>),
}
