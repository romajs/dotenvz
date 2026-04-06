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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::mock::InMemoryProvider;

    #[test]
    fn resolve_env_returns_all_provider_secrets() {
        let p = InMemoryProvider::new();
        p.set_secret("app", "dev", "DB_URL", "postgres://localhost")
            .unwrap();
        p.set_secret("app", "dev", "PORT", "5432").unwrap();
        p.set_secret("app", "prod", "DB_URL", "postgres://prod")
            .unwrap(); // different profile

        let env = resolve_env(&p, "app", "dev").unwrap();
        assert_eq!(env.len(), 2);
        assert_eq!(env.get("DB_URL").unwrap(), "postgres://localhost");
        assert_eq!(env.get("PORT").unwrap(), "5432");
        assert!(!env.contains_key("prod_key"));
    }

    #[test]
    fn resolve_env_returns_empty_for_empty_store() {
        let p = InMemoryProvider::new();
        let env = resolve_env(&p, "app", "dev").unwrap();
        assert!(env.is_empty());
    }
}
