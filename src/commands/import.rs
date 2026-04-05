use crate::core::project_context::ProjectContext;
use crate::errors::{DotenvzError, Result};
use crate::providers::secret_provider::SecretProvider;

/// Import variables from a `.env` file into the secret provider for the active profile.
///
/// The `.env` file is parsed and each key/value pair is stored via the provider.
/// This is a one-time bootstrap operation — the file is not used at runtime.
pub fn run(
    ctx: &ProjectContext,
    provider: &dyn SecretProvider,
    file_override: Option<&str>,
) -> Result<()> {
    let import_path = ctx
        .project_dir()
        .join(file_override.unwrap_or(&ctx.config.import_file));

    if !import_path.exists() {
        return Err(DotenvzError::Import(format!(
            "File not found: {}",
            import_path.display()
        )));
    }

    println!("Importing from {} ...", import_path.display());

    let vars = dotenvy::from_path_iter(&import_path)
        .map_err(|e| DotenvzError::Import(e.to_string()))?;

    let mut count = 0usize;
    for item in vars {
        let (key, value) = item.map_err(|e| DotenvzError::Import(e.to_string()))?;
        provider.set_secret(&ctx.config.project, &ctx.profile, &key, &value)?;
        count += 1;
    }

    println!(
        "✓ Imported {count} variable(s) into project '{}' profile '{}'",
        ctx.config.project, ctx.profile
    );
    Ok(())
}
