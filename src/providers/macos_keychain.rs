//! macOS Keychain provider — stub implementation.
//!
//! Secrets are stored as **Generic Password** items in the user's login Keychain:
//!
//! | Keychain field | dotenvz value              |
//! |----------------|----------------------------|
//! | `kSecAttrService` | `dotenvz.<project>.<profile>` |
//! | `kSecAttrAccount` | `<key>`                   |
//! | `kSecValueData`   | UTF-8 encoded `<value>`   |
//!
//! The full implementation will use the [`security-framework`] crate which
//! provides safe bindings to macOS Security.framework.
//!
//! [`security-framework`]: https://docs.rs/security-framework

use std::collections::HashMap;

use crate::errors::{DotenvzError, Result};
use crate::providers::secret_provider::SecretProvider;

/// Secret provider backed by the macOS Keychain.
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

impl SecretProvider for MacOsKeychainProvider {
    fn set_secret(&self, project: &str, profile: &str, key: &str, _value: &str) -> Result<()> {
        let _service = Self::service_name(project, profile);
        // TODO: implement using security_framework::passwords::set_generic_password(
        //     &service, key, _value.as_bytes()
        // )
        Err(DotenvzError::Provider(
            "macOS Keychain provider is not yet implemented".into(),
        ))
    }

    fn get_secret(&self, project: &str, profile: &str, key: &str) -> Result<String> {
        let _service = Self::service_name(project, profile);
        // TODO: implement using security_framework::passwords::get_generic_password(
        //     &service, key
        // ) and convert the returned Vec<u8> to a UTF-8 String.
        Err(DotenvzError::KeyNotFound {
            key: key.to_string(),
            profile: profile.to_string(),
        })
    }

    fn list_secrets(&self, project: &str, profile: &str) -> Result<HashMap<String, String>> {
        let _service = Self::service_name(project, profile);
        // TODO: implement using security_framework::item::ItemSearchOptions to query
        // all Generic Password items whose kSecAttrService matches the service name,
        // then collect kSecAttrAccount → kSecValueData pairs into the map.
        Err(DotenvzError::Provider(
            "macOS Keychain list is not yet implemented".into(),
        ))
    }

    fn delete_secret(&self, project: &str, profile: &str, key: &str) -> Result<()> {
        let _service = Self::service_name(project, profile);
        // TODO: implement using security_framework::item::ItemSearchOptions to locate
        // the item and then call delete() on the result.
        Err(DotenvzError::KeyNotFound {
            key: key.to_string(),
            profile: profile.to_string(),
        })
    }
}
