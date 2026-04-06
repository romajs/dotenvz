use crate::core::project_context::ProjectContext;
use crate::errors::Result;
use crate::providers::secret_provider::SecretProvider;

/// Remove a secret from the provider for the active profile.
pub fn run(ctx: &ProjectContext, provider: &dyn SecretProvider, key: &str) -> Result<()> {
    provider.delete_secret(&ctx.config.project, &ctx.profile, key)?;
    println!("✓ Removed '{key}' from profile '{}'", ctx.profile);
    Ok(())
}
