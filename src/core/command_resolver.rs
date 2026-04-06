use crate::config::DotenvzConfig;

/// Names of every built-in command handled directly by the CLI.
pub const BUILTIN_COMMANDS: &[&str] = &["init", "import", "set", "get", "list", "rm", "exec"];

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn config_with_commands(cmds: &[(&str, &str)]) -> DotenvzConfig {
        let mut commands = HashMap::new();
        for (k, v) in cmds {
            commands.insert(k.to_string(), v.to_string());
        }
        DotenvzConfig {
            project: "test".into(),
            provider: "macos-keychain".into(),
            default_profile: "dev".into(),
            schema_file: None,
            import_file: ".env".into(),
            commands,
        }
    }

    #[test]
    fn recognizes_all_builtins() {
        for cmd in BUILTIN_COMMANDS {
            let result = resolve_command(cmd, None);
            assert!(
                matches!(result, Some(ResolvedCommand::Builtin(_))),
                "`{cmd}` should be a builtin"
            );
        }
    }

    #[test]
    fn builtin_takes_priority_over_alias() {
        // Even if a user named an alias "init", the builtin wins.
        let cfg = config_with_commands(&[("init", "echo override")]);
        let result = resolve_command("init", Some(&cfg));
        assert!(matches!(result, Some(ResolvedCommand::Builtin(_))));
    }

    #[test]
    fn resolves_alias_from_config() {
        let cfg = config_with_commands(&[("dev", "next dev"), ("build", "next build")]);

        let result = resolve_command("dev", Some(&cfg)).unwrap();
        assert!(matches!(
            &result,
            ResolvedCommand::Alias { name, resolved }
            if name == "dev" && resolved == "next dev"
        ));
    }

    #[test]
    fn returns_none_for_unknown_command() {
        let cfg = config_with_commands(&[("dev", "next dev")]);
        assert!(resolve_command("unknown", Some(&cfg)).is_none());
        assert!(resolve_command("unknown", None).is_none());
    }

    #[test]
    fn no_config_resolves_only_builtins() {
        assert!(resolve_command("set", None).is_some());
        assert!(resolve_command("dev", None).is_none());
    }
}
