use std::collections::HashMap;
use std::sync::Mutex;

use crate::errors::{DotenvzError, Result};
use crate::providers::secret_provider::SecretProvider;

/// Thread-safe in-memory secret provider backed by a `HashMap`.
///
/// Used in unit and integration tests to avoid requiring a real Keychain.
/// The internal key format is `"{project}:{profile}:{key}"`.
pub struct InMemoryProvider {
    store: Mutex<HashMap<String, String>>,
}

impl InMemoryProvider {
    pub fn new() -> Self {
        Self {
            store: Mutex::new(HashMap::new()),
        }
    }

    fn make_key(project: &str, profile: &str, key: &str) -> String {
        format!("{project}:{profile}:{key}")
    }
}

impl Default for InMemoryProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretProvider for InMemoryProvider {
    fn set_secret(&self, project: &str, profile: &str, key: &str, value: &str) -> Result<()> {
        let k = Self::make_key(project, profile, key);
        self.store.lock().unwrap().insert(k, value.to_string());
        Ok(())
    }

    fn get_secret(&self, project: &str, profile: &str, key: &str) -> Result<String> {
        let k = Self::make_key(project, profile, key);
        self.store
            .lock()
            .unwrap()
            .get(&k)
            .cloned()
            .ok_or_else(|| DotenvzError::KeyNotFound {
                key: key.to_string(),
                profile: profile.to_string(),
            })
    }

    fn list_secrets(&self, project: &str, profile: &str) -> Result<HashMap<String, String>> {
        let prefix = format!("{project}:{profile}:");
        let map = self
            .store
            .lock()
            .unwrap()
            .iter()
            .filter(|(k, _)| k.starts_with(&prefix))
            .map(|(k, v)| (k[prefix.len()..].to_string(), v.clone()))
            .collect();
        Ok(map)
    }

    fn delete_secret(&self, project: &str, profile: &str, key: &str) -> Result<()> {
        let k = Self::make_key(project, profile, key);
        let removed = self.store.lock().unwrap().remove(&k);
        removed
            .map(|_| ())
            .ok_or_else(|| DotenvzError::KeyNotFound {
                key: key.to_string(),
                profile: profile.to_string(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> InMemoryProvider {
        InMemoryProvider::new()
    }

    #[test]
    fn set_and_get_round_trip() {
        let p = provider();
        p.set_secret("proj", "dev", "DB_URL", "postgres://localhost")
            .unwrap();
        let v = p.get_secret("proj", "dev", "DB_URL").unwrap();
        assert_eq!(v, "postgres://localhost");
    }

    #[test]
    fn set_overwrites_existing_value() {
        let p = provider();
        p.set_secret("proj", "dev", "K", "old").unwrap();
        p.set_secret("proj", "dev", "K", "new").unwrap();
        assert_eq!(p.get_secret("proj", "dev", "K").unwrap(), "new");
    }

    #[test]
    fn get_missing_key_returns_key_not_found() {
        let p = provider();
        let err = p.get_secret("proj", "dev", "MISSING").unwrap_err();
        assert!(matches!(err, DotenvzError::KeyNotFound { key, .. } if key == "MISSING"));
    }

    #[test]
    fn list_secrets_scoped_by_project_and_profile() {
        let p = provider();
        p.set_secret("proj", "dev", "A", "1").unwrap();
        p.set_secret("proj", "dev", "B", "2").unwrap();
        // Different profile — should NOT appear in "dev" listing.
        p.set_secret("proj", "prod", "A", "prod-val").unwrap();
        // Different project — should NOT appear either.
        p.set_secret("other", "dev", "A", "other-val").unwrap();

        let secrets = p.list_secrets("proj", "dev").unwrap();
        assert_eq!(secrets.len(), 2);
        assert_eq!(secrets.get("A"), Some(&"1".to_string()));
        assert_eq!(secrets.get("B"), Some(&"2".to_string()));
    }

    #[test]
    fn list_secrets_returns_empty_when_none_set() {
        let p = provider();
        let secrets = p.list_secrets("proj", "dev").unwrap();
        assert!(secrets.is_empty());
    }

    #[test]
    fn delete_removes_secret() {
        let p = provider();
        p.set_secret("proj", "dev", "K", "v").unwrap();
        p.delete_secret("proj", "dev", "K").unwrap();
        assert!(p.get_secret("proj", "dev", "K").is_err());
    }

    #[test]
    fn delete_missing_key_returns_key_not_found() {
        let p = provider();
        let err = p.delete_secret("proj", "dev", "GHOST").unwrap_err();
        assert!(matches!(err, DotenvzError::KeyNotFound { .. }));
    }

    #[test]
    fn profiles_are_isolated() {
        let p = provider();
        p.set_secret("proj", "dev", "KEY", "dev-val").unwrap();
        p.set_secret("proj", "staging", "KEY", "staging-val")
            .unwrap();
        assert_eq!(p.get_secret("proj", "dev", "KEY").unwrap(), "dev-val");
        assert_eq!(
            p.get_secret("proj", "staging", "KEY").unwrap(),
            "staging-val"
        );
    }
}
