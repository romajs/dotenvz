use std::collections::HashMap;

use crate::errors::Result;
use crate::providers::secret_provider::SecretProvider;

/// Build the environment map that will be injected into a child process.
///
/// Fetches all secrets for the given project and profile from the provider.
///
/// # Future work
/// - Optionally validate fetched keys against `schema_file`.
/// - Support merging a base profile (e.g. `dev`) with an overlay profile.
pub fn resolve_env(
    provider: &dyn SecretProvider,
    project: &str,
    profile: &str,
) -> Result<HashMap<String, String>> {
    // TODO: filter by schema_file keys when schema validation is implemented.
    provider.list_secrets(project, profile)
}
