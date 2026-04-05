use crate::core::project_context::ProjectContext;
use crate::errors::Result;
use crate::providers::secret_provider::SecretProvider;

/// List all secret keys for the current project and profile.
pub fn run(ctx: &ProjectContext, provider: &dyn SecretProvider) -> Result<()> {
    let secrets = provider.list_secrets(&ctx.config.project, &ctx.profile)?;

    if secrets.is_empty() {
        println!(
            "No secrets found for project '{}' profile '{}'.",
            ctx.config.project, ctx.profile
        );
        return Ok(());
    }

    println!(
        "Secrets — project: '{}', profile: '{}'",
        ctx.config.project, ctx.profile
    );
    let mut keys: Vec<&String> = secrets.keys().collect();
    keys.sort();
    for key in keys {
        println!("  {key}");
    }
    Ok(())
}
