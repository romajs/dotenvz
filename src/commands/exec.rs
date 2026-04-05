use crate::core::{env_resolver, process_runner};
use crate::core::project_context::ProjectContext;
use crate::errors::{DotenvzError, Result};
use crate::providers::secret_provider::SecretProvider;

/// Execute an arbitrary command with secrets injected as environment variables.
///
/// Usage: `dotenvz exec -- <command> [args...]`
///
/// Secrets for the active project and profile are fetched from the provider
/// and overlaid on top of the current process environment before execution.
pub fn run(
    ctx: &ProjectContext,
    provider: &dyn SecretProvider,
    args: &[String],
) -> Result<()> {
    if args.is_empty() {
        return Err(DotenvzError::ProcessExec(
            "No command provided. Usage: dotenvz exec -- <command> [args...]".into(),
        ));
    }

    let env = env_resolver::resolve_env(provider, &ctx.config.project, &ctx.profile)?;

    let program = &args[0];
    let rest: Vec<&str> = args[1..].iter().map(String::as_str).collect();
    process_runner::run_process(program, &rest, &env)
}
