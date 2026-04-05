use std::collections::HashMap;

use crate::errors::Result;

/// A secret value scoped by project, profile, and key.
#[derive(Debug, Clone)]
pub struct Secret {
    pub project: String,
    pub profile: String,
    pub key: String,
    pub value: String,
}

/// Backend abstraction for secret storage.
///
/// All operations are namespaced by `project` and `profile` so that a key
/// such as `DATABASE_URL` can exist independently across environments
/// (e.g. `dev`, `staging`, `production`).
///
/// Implementors must be `Send + Sync` to allow future async or threaded use.
pub trait SecretProvider: Send + Sync {
    /// Store or overwrite a secret value.
    fn set_secret(&self, project: &str, profile: &str, key: &str, value: &str) -> Result<()>;

    /// Retrieve a single secret value by key.
    ///
    /// Returns `DotenvzError::KeyNotFound` when the key does not exist.
    fn get_secret(&self, project: &str, profile: &str, key: &str) -> Result<String>;

    /// Return all secrets for a project/profile as a `key → value` map.
    ///
    /// Used by the env resolver to build the environment for process injection.
    fn list_secrets(&self, project: &str, profile: &str) -> Result<HashMap<String, String>>;

    /// Permanently delete a secret by key.
    ///
    /// Returns `DotenvzError::KeyNotFound` when the key does not exist.
    fn delete_secret(&self, project: &str, profile: &str, key: &str) -> Result<()>;
}
