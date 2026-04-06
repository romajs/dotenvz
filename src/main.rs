use clap::Parser;
use dotenvz::{
    cli::{Cli, Commands},
    commands,
    core::{
        command_resolver::{resolve_command, ResolvedCommand},
        env_resolver, process_runner,
        project_context::ProjectContext,
    },
    errors::{self, DotenvzError},
    providers::secret_provider::SecretProvider,
};

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

/// Instantiate the active secret provider for the current platform.
fn build_provider() -> errors::Result<Box<dyn SecretProvider>> {
    #[cfg(target_os = "macos")]
    {
        use dotenvz::providers::macos_keychain::MacOsKeychainProvider;
        Ok(Box::new(MacOsKeychainProvider::new()))
    }
    #[cfg(target_os = "linux")]
    {
        use dotenvz::providers::linux_secret_service::LinuxSecretServiceProvider;
        Ok(Box::new(LinuxSecretServiceProvider::new()))
    }
    #[cfg(target_os = "windows")]
    {
        use dotenvz::providers::windows_credential::WindowsCredentialProvider;
        Ok(Box::new(WindowsCredentialProvider::new()))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    Err(DotenvzError::UnsupportedPlatform)
}

fn run() -> errors::Result<()> {
    let cli = Cli::parse();

    // `init` does not require a loaded project context or provider.
    if let Commands::Init { force } = cli.command {
        return commands::init::run(None, force);
    }

    // All other commands require a resolved project context.
    let ctx = ProjectContext::resolve(cli.profile.as_deref())?;

    // Build the active secret provider.
    let provider = build_provider()?;

    match cli.command {
        Commands::Init { .. } => unreachable!("handled above"),

        Commands::Import { file } => {
            commands::import::run(&ctx, provider.as_ref(), file.as_deref(), cli.dry_run)
        }

        Commands::Set { key, value } => commands::set::run(&ctx, provider.as_ref(), &key, &value),

        Commands::Get { key } => commands::get::run(&ctx, provider.as_ref(), &key),

        Commands::List => commands::list::run(&ctx, provider.as_ref()),

        Commands::Rm { key } => commands::rm::run(&ctx, provider.as_ref(), &key),

        Commands::Exec { args } => commands::exec::run(&ctx, provider.as_ref(), &args, cli.dry_run),

        // Alias: first arg is the alias name; remaining args are forwarded.
        Commands::Alias(parts) => {
            let alias_name = parts
                .first()
                .ok_or_else(|| DotenvzError::UnknownCommand("<empty>".into()))?;

            match resolve_command(alias_name, Some(&ctx.config)) {
                Some(ResolvedCommand::Alias { resolved, .. }) => {
                    if cli.dry_run {
                        println!("[dry-run] alias `{alias_name}` → `{resolved}`");
                        let env = env_resolver::resolve_env(
                            provider.as_ref(),
                            &ctx.config.project,
                            &ctx.profile,
                        )?;
                        let mut keys: Vec<_> = env.keys().collect();
                        keys.sort();
                        for key in keys {
                            println!("  {key}=<redacted>");
                        }
                        Ok(())
                    } else {
                        let env = env_resolver::resolve_env(
                            provider.as_ref(),
                            &ctx.config.project,
                            &ctx.profile,
                        )?;
                        process_runner::run_command_string(&resolved, &env)
                    }
                }
                _ => Err(DotenvzError::UnknownCommand(alias_name.clone())),
            }
        }
    }
}
