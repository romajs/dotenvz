use crate::core::project_context::ProjectContext;
use crate::errors::Result;
use crate::providers::secret_provider::SecretProvider;

/// Store or update a secret in the provider for the active profile.
pub fn run(
    ctx: &ProjectContext,
    provider: &dyn SecretProvider,
    key: &str,
    value: &str,
) -> Result<()> {
    provider.set_secret(&ctx.config.project, &ctx.profile, key, value)?;
    println!("✓ Set '{key}' in profile '{}'", ctx.profile);
    Ok(())
}
