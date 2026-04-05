//! macOS Keychain provider — full implementation using `security-framework`.
//!
//! ## Secret storage layout
//!
//! Each secret is stored as a **Generic Password** item:
//!
//! | Keychain attribute        | Value                          |
//! |---------------------------|--------------------------------|
//! | `kSecAttrService`         | `dotenvz.<project>.<profile>`  |
//! | `kSecAttrAccount`         | `<key>`                        |
//! | `kSecValueData`           | UTF-8 encoded `<value>`        |
//!
//! ## Key registry
//!
//! macOS Keychain does not expose a clean "list all accounts for a service"
//! API without CoreFoundation type-casting. To implement `list_secrets`
//! efficiently, this provider maintains a **key registry**: a single
//! newline-delimited list of key names stored under the account
//! `"__dotenvz_idx__"` in the same service namespace.
//!
//! The registry is updated on every `set_secret` and `delete_secret` call.
//! If an item is removed directly from Keychain Access, the registry may drift;
//! this is an acceptable trade-off for an MVP CLI.

use std::collections::HashMap;

use crate::errors::{DotenvzError, Result};
use crate::providers::secret_provider::SecretProvider;

/// The account name used to store the list of registered secret keys.
const REGISTRY_ACCOUNT: &str = "__dotenvz_idx__";

/// macOS errSecItemNotFound status code.
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
/// macOS errSecDuplicateItem status code.
const ERR_SEC_DUPLICATE_ITEM: i32 = -25299;

/// Secret provider backed by the macOS login Keychain.
pub struct MacOsKeychainProvider;

impl MacOsKeychainProvider {
    pub fn new() -> Self {
        Self
    }

    /// Build the Keychain service name from project and profile.
    ///
    /// Format: `dotenvz.<project>.<profile>`
    fn service_name(project: &str, profile: &str) -> String {
        format!("dotenvz.{project}.{profile}")
    }
}

impl Default for MacOsKeychainProvider {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Registry helpers (macOS only)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
impl MacOsKeychainProvider {
    /// Read the current list of registered key names for a service.
    fn read_registry(service: &str) -> Vec<String> {
        match security_framework::passwords::get_generic_password(service, REGISTRY_ACCOUNT) {
            Ok(bytes) => String::from_utf8(bytes)
                .unwrap_or_default()
                .lines()
                .filter(|l| !l.is_empty())
                .map(str::to_owned)
                .collect(),
            Err(_) => vec![],
        }
    }

    /// Persist the registry list to Keychain, creating or updating the entry.
    fn write_registry(service: &str, keys: &[String]) -> Result<()> {
        let data = keys.join("\n");
        upsert_password(service, REGISTRY_ACCOUNT, data.as_bytes())
    }

    /// Add `key` to the registry if it is not already present.
    fn registry_add(service: &str, key: &str) -> Result<()> {
        let mut keys = Self::read_registry(service);
        if !keys.iter().any(|k| k == key) {
            keys.push(key.to_owned());
            Self::write_registry(service, &keys)?;
        }
        Ok(())
    }

    /// Remove `key` from the registry.
    fn registry_remove(service: &str, key: &str) -> Result<()> {
        let keys: Vec<String> = Self::read_registry(service)
            .into_iter()
            .filter(|k| k != key)
            .collect();
        Self::write_registry(service, &keys)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Upsert helper — handles errSecDuplicateItem transparently
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn upsert_password(service: &str, account: &str, password: &[u8]) -> Result<()> {
    use security_framework::passwords::{delete_generic_password, set_generic_password};

    match set_generic_password(service, account, password) {
        Ok(()) => Ok(()),
        Err(e) if e.code() == ERR_SEC_DUPLICATE_ITEM => {
            // Item already exists — delete then re-add atomically enough for a CLI.
            let _ = delete_generic_password(service, account);
            set_generic_password(service, account, password)
                .map_err(|e| DotenvzError::Provider(e.to_string()))
        }
        Err(e) => Err(DotenvzError::Provider(e.to_string())),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SecretProvider implementation
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "macos"))]
impl SecretProvider for MacOsKeychainProvider {
    fn set_secret(&self, _: &str, _: &str, _: &str, _: &str) -> Result<()> {
        Err(DotenvzError::UnsupportedPlatform)
    }
    fn get_secret(&self, _: &str, _: &str, key: &str) -> Result<String> {
        Err(DotenvzError::KeyNotFound {
            key: key.to_string(),
            profile: String::new(),
        })
    }
    fn list_secrets(&self, _: &str, _: &str) -> Result<HashMap<String, String>> {
        Err(DotenvzError::UnsupportedPlatform)
    }
    fn delete_secret(&self, _: &str, _: &str, key: &str) -> Result<()> {
        Err(DotenvzError::KeyNotFound {
            key: key.to_string(),
            profile: String::new(),
        })
    }
}

#[cfg(target_os = "macos")]
impl SecretProvider for MacOsKeychainProvider {
    fn set_secret(&self, project: &str, profile: &str, key: &str, value: &str) -> Result<()> {
        let service = Self::service_name(project, profile);
        upsert_password(&service, key, value.as_bytes())?;
        Self::registry_add(&service, key)
    }

    fn get_secret(&self, project: &str, profile: &str, key: &str) -> Result<String> {
        let service = Self::service_name(project, profile);
        security_framework::passwords::get_generic_password(&service, key)
            .map_err(|e| {
                if e.code() == ERR_SEC_ITEM_NOT_FOUND {
                    DotenvzError::KeyNotFound {
                        key: key.to_string(),
                        profile: profile.to_string(),
                    }
                } else {
                    DotenvzError::Provider(e.to_string())
                }
            })
            .and_then(|bytes| {
                String::from_utf8(bytes)
                    .map_err(|e| DotenvzError::Provider(format!("Invalid UTF-8 in secret `{key}`: {e}")))
            })
    }

    fn list_secrets(&self, project: &str, profile: &str) -> Result<HashMap<String, String>> {
        let service = Self::service_name(project, profile);
        let keys = Self::read_registry(&service);

        let mut map = HashMap::new();
        for key in keys {
            // Skip the internal registry entry itself.
            if key == REGISTRY_ACCOUNT {
                continue;
            }
            match self.get_secret(project, profile, &key) {
                Ok(value) => {
                    map.insert(key, value);
                }
                Err(DotenvzError::KeyNotFound { .. }) => {
                    // Key is in registry but not in Keychain — registry is stale; skip.
                }
                Err(e) => return Err(e),
            }
        }
        Ok(map)
    }

    fn delete_secret(&self, project: &str, profile: &str, key: &str) -> Result<()> {
        let service = Self::service_name(project, profile);
        security_framework::passwords::delete_generic_password(&service, key)
            .map_err(|e| {
                if e.code() == ERR_SEC_ITEM_NOT_FOUND {
                    DotenvzError::KeyNotFound {
                        key: key.to_string(),
                        profile: profile.to_string(),
                    }
                } else {
                    DotenvzError::Provider(e.to_string())
                }
            })?;
        Self::registry_remove(&service, key)
    }
}
