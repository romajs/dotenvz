mod cli;
mod commands;
mod config;
mod core;
mod errors;
mod providers;

use clap::Parser;
use cli::{Cli, Commands};
use crate::core::command_resolver::{resolve_command, ResolvedCommand};
use crate::core::{env_resolver, process_runner};
use crate::core::project_context::ProjectContext;
use errors::DotenvzError;
use providers::secret_provider::SecretProvider;

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

/// Instantiate the active secret provider based on the current platform.
///
/// Only macOS / Apple Keychain is supported in this MVP.
fn build_provider() -> errors::Result<Box<dyn SecretProvider>> {
    #[cfg(not(target_os = "macos"))]
    return Err(DotenvzError::UnsupportedPlatform);

    #[cfg(target_os = "macos")]
    {
        use providers::macos_keychain::MacOsKeychainProvider;
        Ok(Box::new(MacOsKeychainProvider::new()))
    }
}

fn run() -> errors::Result<()> {
    let cli = Cli::parse();

    // `init` does not require a loaded project context or provider.
    if let Commands::Init = cli.command {
        return commands::init::run(None);
    }

    // All other commands require a resolved project context.
    let ctx = ProjectContext::resolve(cli.profile.as_deref())?;

    // Build the active secret provider.
    let provider = build_provider()?;

    match cli.command {
        Commands::Init => unreachable!("handled above"),

        Commands::Import { file } => {
            commands::import::run(&ctx, provider.as_ref(), file.as_deref())
        }

        Commands::Set { key, value } => {
            commands::set::run(&ctx, provider.as_ref(), &key, &value)
        }

        Commands::Get { key } => commands::get::run(&ctx, provider.as_ref(), &key),

        Commands::List => commands::list::run(&ctx, provider.as_ref()),

        Commands::Rm { key } => commands::rm::run(&ctx, provider.as_ref(), &key),

        Commands::Exec { args } => commands::exec::run(&ctx, provider.as_ref(), &args),

        // Alias: first arg is the alias name; remaining args are forwarded.
        Commands::Alias(parts) => {
            let alias_name = parts.first().ok_or_else(|| {
                DotenvzError::UnknownCommand("<empty>".into())
            })?;

            match resolve_command(alias_name, Some(&ctx.config)) {
                Some(ResolvedCommand::Alias { resolved, .. }) => {
                    let env = env_resolver::resolve_env(
                        provider.as_ref(),
                        &ctx.config.project,
                        &ctx.profile,
                    )?;
                    process_runner::run_command_string(&resolved, &env)
                }
                _ => Err(DotenvzError::UnknownCommand(alias_name.clone())),
            }
        }
    }
}
