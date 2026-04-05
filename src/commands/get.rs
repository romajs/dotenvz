use crate::core::project_context::ProjectContext;
use crate::errors::Result;
use crate::providers::secret_provider::SecretProvider;

/// Retrieve and print a single secret value from the provider.
pub fn run(
    ctx: &ProjectContext,
    provider: &dyn SecretProvider,
    key: &str,
) -> Result<()> {
    let value = provider.get_secret(&ctx.config.project, &ctx.profile, key)?;
    println!("{value}");
    Ok(())
}
