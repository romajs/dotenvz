use crate::core::project_context::ProjectContext;
use crate::errors::{DotenvzError, Result};
use crate::providers::secret_provider::SecretProvider;

/// Import variables from a `.env` file into the secret provider for the active profile.
///
/// Variables with empty values are skipped with a warning.
/// Pass `dry_run = true` to preview what would be imported without making changes.
///
/// This is a one-time bootstrap operation — the `.env` file is never used at runtime.
pub fn run(
    ctx: &ProjectContext,
    provider: &dyn SecretProvider,
    file_override: Option<&str>,
    dry_run: bool,
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

    if dry_run {
        println!("[dry-run] Would import from: {}", import_path.display());
    } else {
        println!("Importing from {} ...", import_path.display());
    }

    let vars =
        dotenvy::from_path_iter(&import_path).map_err(|e| DotenvzError::Import(e.to_string()))?;

    let mut imported = 0usize;
    let mut skipped = 0usize;

    for item in vars {
        let (key, value) = item.map_err(|e| DotenvzError::Import(e.to_string()))?;

        if value.trim().is_empty() {
            eprintln!("  skip: {key} (empty value)");
            skipped += 1;
            continue;
        }

        if dry_run {
            println!("  [dry-run] would set: {key}");
        } else {
            provider.set_secret(&ctx.config.project, &ctx.profile, &key, &value)?;
        }
        imported += 1;
    }

    if dry_run {
        println!(
            "[dry-run] would import {imported} variable(s) ({skipped} skipped with empty value)"
        );
    } else {
        println!(
            "✓ Imported {imported} variable(s) into project '{}' profile '{}' ({skipped} skipped)",
            ctx.config.project, ctx.profile
        );
    }

    Ok(())
}
