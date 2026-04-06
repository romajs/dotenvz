use crate::core::project_context::ProjectContext;
use crate::core::{env_resolver, process_runner};
use crate::errors::{DotenvzError, Result};
use crate::providers::secret_provider::SecretProvider;

/// Execute an arbitrary command with secrets injected as environment variables.
///
/// Usage: `dotenvz exec -- <command> [args...]`
///
/// Pass `dry_run = true` to print the resolved env keys and command without
/// actually executing anything.
pub fn run(
    ctx: &ProjectContext,
    provider: &dyn SecretProvider,
    args: &[String],
    dry_run: bool,
) -> Result<()> {
    if args.is_empty() {
        return Err(DotenvzError::ProcessExec(
            "No command provided. Usage: dotenvz exec -- <command> [args...]".into(),
        ));
    }

    let env = env_resolver::resolve_env(provider, &ctx.config.project, &ctx.profile)?;

    if dry_run {
        println!(
            "[dry-run] Would inject {} secret(s) from project '{}' profile '{}':",
            env.len(),
            ctx.config.project,
            ctx.profile
        );
        let mut keys: Vec<&String> = env.keys().collect();
        keys.sort();
        for key in keys {
            println!("  {key}=<redacted>");
        }
        println!("[dry-run] Command: {}", args.join(" "));
        return Ok(());
    }

    let program = &args[0];
    let rest: Vec<&str> = args[1..].iter().map(String::as_str).collect();
    process_runner::run_process(program, &rest, &env)
}
