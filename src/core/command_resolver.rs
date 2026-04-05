use crate::config::DotenvzConfig;

/// Names of every built-in command handled directly by the CLI.
pub const BUILTIN_COMMANDS: &[&str] =
    &["init", "import", "set", "get", "list", "rm", "exec"];

/// The result of resolving a top-level CLI argument.
#[derive(Debug, Clone)]
pub enum ResolvedCommand {
    /// A recognized built-in (dispatched by the CLI match).
    Builtin(String),
    /// A `[commands]` alias from `.dotenvz.toml`, resolved to a shell string.
    Alias { name: String, resolved: String },
}

/// Determine whether `name` is a built-in command or a config alias.
///
/// Returns `None` if the name is neither, indicating an unknown command.
pub fn resolve_command(name: &str, config: Option<&DotenvzConfig>) -> Option<ResolvedCommand> {
    if BUILTIN_COMMANDS.contains(&name) {
        return Some(ResolvedCommand::Builtin(name.to_string()));
    }

    if let Some(cfg) = config {
        if let Some(cmd) = cfg.commands.get(name) {
            return Some(ResolvedCommand::Alias {
                name: name.to_string(),
                resolved: cmd.clone(),
            });
        }
    }

    None
}
