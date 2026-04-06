//! Linux Secret Service provider.
//!
//! Stores secrets as items in the default GNOME Keyring / KWallet collection
//! using the Secret Service D-Bus API (via `secret-service` crate).
//!
//! ## Secret storage layout
//!
//! Each item carries four attributes:
//!
//! | Attribute     | Value                         |
//! |---------------|-------------------------------|
//! | `application` | `"dotenvz"` (constant)        |
//! | `project`     | project name from config      |
//! | `profile`     | active profile name           |
//! | `key`         | env-var key name              |
//!
//! The label format is `dotenvz/<project>/<profile>/<key>`.
//!
//! Because Secret Service supports attribute-based search natively, no
//! separate key registry (like the macOS sentinel account) is required.

/// Secret provider backed by the Linux Secret Service D-Bus API.
pub struct LinuxSecretServiceProvider;

impl LinuxSecretServiceProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LinuxSecretServiceProvider {
    fn default() -> Self {
        Self::new()
    }
}

// ── Linux implementation ────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
impl crate::providers::secret_provider::SecretProvider for LinuxSecretServiceProvider {
    fn set_secret(
        &self,
        project: &str,
        profile: &str,
        key: &str,
        value: &str,
    ) -> crate::errors::Result<()> {
        use secret_service::blocking::SecretService;
        use secret_service::EncryptionType;
        use std::collections::HashMap;

        let ss = SecretService::connect(EncryptionType::Dh)
            .map_err(|e| crate::errors::DotenvzError::Provider(e.to_string()))?;
        let collection = ss
            .get_default_collection()
            .map_err(|e| crate::errors::DotenvzError::Provider(e.to_string()))?;
        collection
            .ensure_unlocked()
            .map_err(|e| crate::errors::DotenvzError::Provider(e.to_string()))?;

        let mut attrs = HashMap::new();
        attrs.insert("application", "dotenvz");
        attrs.insert("project", project);
        attrs.insert("profile", profile);
        attrs.insert("key", key);

        collection
            .create_item(
                &format!("dotenvz/{project}/{profile}/{key}"),
                attrs,
                value.as_bytes(),
                true, // replace existing item with same attributes
                "text/plain",
            )
            .map_err(|e| crate::errors::DotenvzError::Provider(e.to_string()))?;

        Ok(())
    }

    fn get_secret(&self, project: &str, profile: &str, key: &str) -> crate::errors::Result<String> {
        use secret_service::blocking::SecretService;
        use secret_service::EncryptionType;
        use std::collections::HashMap;

        let ss = SecretService::connect(EncryptionType::Dh)
            .map_err(|e| crate::errors::DotenvzError::Provider(e.to_string()))?;
        let collection = ss
            .get_default_collection()
            .map_err(|e| crate::errors::DotenvzError::Provider(e.to_string()))?;
        collection
            .ensure_unlocked()
            .map_err(|e| crate::errors::DotenvzError::Provider(e.to_string()))?;

        let mut attrs = HashMap::new();
        attrs.insert("application", "dotenvz");
        attrs.insert("project", project);
        attrs.insert("profile", profile);
        attrs.insert("key", key);

        let items = collection
            .search_items(attrs)
            .map_err(|e| crate::errors::DotenvzError::Provider(e.to_string()))?;

        let item =
            items
                .into_iter()
                .next()
                .ok_or_else(|| crate::errors::DotenvzError::KeyNotFound {
                    key: key.to_string(),
                    profile: profile.to_string(),
                })?;

        item.ensure_unlocked()
            .map_err(|e| crate::errors::DotenvzError::Provider(e.to_string()))?;

        let bytes = item
            .get_secret()
            .map_err(|e| crate::errors::DotenvzError::Provider(e.to_string()))?;

        String::from_utf8(bytes).map_err(|e| {
            crate::errors::DotenvzError::Provider(format!("secret is not valid UTF-8: {e}"))
        })
    }

    fn list_secrets(
        &self,
        project: &str,
        profile: &str,
    ) -> crate::errors::Result<std::collections::HashMap<String, String>> {
        use secret_service::blocking::SecretService;
        use secret_service::EncryptionType;
        use std::collections::HashMap;

        let ss = SecretService::connect(EncryptionType::Dh)
            .map_err(|e| crate::errors::DotenvzError::Provider(e.to_string()))?;
        let collection = ss
            .get_default_collection()
            .map_err(|e| crate::errors::DotenvzError::Provider(e.to_string()))?;
        collection
            .ensure_unlocked()
            .map_err(|e| crate::errors::DotenvzError::Provider(e.to_string()))?;

        let mut attrs = HashMap::new();
        attrs.insert("application", "dotenvz");
        attrs.insert("project", project);
        attrs.insert("profile", profile);

        let items = collection
            .search_items(attrs)
            .map_err(|e| crate::errors::DotenvzError::Provider(e.to_string()))?;

        let mut map = HashMap::new();
        for item in items {
            if let Ok(item_attrs) = item.get_attributes() {
                if let Some(k) = item_attrs.get("key") {
                    if let Ok(bytes) = item.get_secret() {
                        if let Ok(v) = String::from_utf8(bytes) {
                            map.insert(k.clone(), v);
                        }
                    }
                }
            }
        }

        Ok(map)
    }

    fn delete_secret(&self, project: &str, profile: &str, key: &str) -> crate::errors::Result<()> {
        use secret_service::blocking::SecretService;
        use secret_service::EncryptionType;
        use std::collections::HashMap;

        let ss = SecretService::connect(EncryptionType::Dh)
            .map_err(|e| crate::errors::DotenvzError::Provider(e.to_string()))?;
        let collection = ss
            .get_default_collection()
            .map_err(|e| crate::errors::DotenvzError::Provider(e.to_string()))?;
        collection
            .ensure_unlocked()
            .map_err(|e| crate::errors::DotenvzError::Provider(e.to_string()))?;

        let mut attrs = HashMap::new();
        attrs.insert("application", "dotenvz");
        attrs.insert("project", project);
        attrs.insert("profile", profile);
        attrs.insert("key", key);

        let items = collection
            .search_items(attrs)
            .map_err(|e| crate::errors::DotenvzError::Provider(e.to_string()))?;

        let item =
            items
                .into_iter()
                .next()
                .ok_or_else(|| crate::errors::DotenvzError::KeyNotFound {
                    key: key.to_string(),
                    profile: profile.to_string(),
                })?;

        item.delete()
            .map_err(|e| crate::errors::DotenvzError::Provider(e.to_string()))
    }
}

// ── Non-Linux stub ──────────────────────────────────────────────────────────

#[cfg(not(target_os = "linux"))]
impl crate::providers::secret_provider::SecretProvider for LinuxSecretServiceProvider {
    fn set_secret(
        &self,
        _project: &str,
        _profile: &str,
        _key: &str,
        _value: &str,
    ) -> crate::errors::Result<()> {
        Err(crate::errors::DotenvzError::UnsupportedPlatform)
    }

    fn get_secret(
        &self,
        _project: &str,
        _profile: &str,
        _key: &str,
    ) -> crate::errors::Result<String> {
        Err(crate::errors::DotenvzError::UnsupportedPlatform)
    }

    fn list_secrets(
        &self,
        _project: &str,
        _profile: &str,
    ) -> crate::errors::Result<std::collections::HashMap<String, String>> {
        Err(crate::errors::DotenvzError::UnsupportedPlatform)
    }

    fn delete_secret(
        &self,
        _project: &str,
        _profile: &str,
        _key: &str,
    ) -> crate::errors::Result<()> {
        Err(crate::errors::DotenvzError::UnsupportedPlatform)
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::secret_provider::SecretProvider;

    // ── Stub tests (compile and run on every non-Linux OS, e.g. macOS CI) ───

    #[cfg(not(target_os = "linux"))]
    mod stub {
        use super::*;
        use crate::errors::DotenvzError;

        fn p() -> LinuxSecretServiceProvider {
            LinuxSecretServiceProvider::new()
        }

        #[test]
        fn set_secret_returns_unsupported_platform() {
            let err = p().set_secret("proj", "dev", "KEY", "val").unwrap_err();
            assert!(matches!(err, DotenvzError::UnsupportedPlatform));
        }

        #[test]
        fn get_secret_returns_unsupported_platform() {
            let err = p().get_secret("proj", "dev", "KEY").unwrap_err();
            assert!(matches!(err, DotenvzError::UnsupportedPlatform));
        }

        #[test]
        fn list_secrets_returns_unsupported_platform() {
            let err = p().list_secrets("proj", "dev").unwrap_err();
            assert!(matches!(err, DotenvzError::UnsupportedPlatform));
        }

        #[test]
        fn delete_secret_returns_unsupported_platform() {
            let err = p().delete_secret("proj", "dev", "KEY").unwrap_err();
            assert!(matches!(err, DotenvzError::UnsupportedPlatform));
        }
    }

    // ── Live tests (Linux only — require a running Secret Service daemon) ───
    // Run with: cargo test -- --include-ignored

    #[cfg(target_os = "linux")]
    mod live {
        use super::*;
        use crate::errors::DotenvzError;

        const PROJECT: &str = "dotenvz-test";
        const PROFILE: &str = "ci";

        fn p() -> LinuxSecretServiceProvider {
            LinuxSecretServiceProvider::new()
        }

        #[test]
        #[ignore = "requires a running Secret Service daemon (gnome-keyring / KWallet)"]
        fn set_get_delete_round_trip() {
            let p = p();
            p.set_secret(PROJECT, PROFILE, "LSS_TEST_KEY", "hello-linux")
                .unwrap();
            let v = p.get_secret(PROJECT, PROFILE, "LSS_TEST_KEY").unwrap();
            assert_eq!(v, "hello-linux");
            p.delete_secret(PROJECT, PROFILE, "LSS_TEST_KEY").unwrap();
            // Verify it is gone.
            assert!(matches!(
                p.get_secret(PROJECT, PROFILE, "LSS_TEST_KEY").unwrap_err(),
                DotenvzError::KeyNotFound { .. }
            ));
        }

        #[test]
        #[ignore = "requires a running Secret Service daemon (gnome-keyring / KWallet)"]
        fn list_secrets_scoped_by_project_and_profile() {
            let p = p();
            p.set_secret(PROJECT, PROFILE, "LSS_A", "v-a").unwrap();
            p.set_secret(PROJECT, PROFILE, "LSS_B", "v-b").unwrap();
            // Different profile — must not appear in PROFILE listing.
            p.set_secret(PROJECT, "other", "LSS_A", "other").unwrap();

            let map = p.list_secrets(PROJECT, PROFILE).unwrap();
            assert_eq!(map.get("LSS_A"), Some(&"v-a".to_string()));
            assert_eq!(map.get("LSS_B"), Some(&"v-b".to_string()));
            assert!(!map.contains_key("other"));

            // Cleanup.
            let _ = p.delete_secret(PROJECT, PROFILE, "LSS_A");
            let _ = p.delete_secret(PROJECT, PROFILE, "LSS_B");
            let _ = p.delete_secret(PROJECT, "other", "LSS_A");
        }

        #[test]
        #[ignore = "requires a running Secret Service daemon (gnome-keyring / KWallet)"]
        fn get_missing_key_returns_key_not_found() {
            let err = p()
                .get_secret(PROJECT, PROFILE, "LSS_NONEXISTENT_XYZ")
                .unwrap_err();
            assert!(matches!(err, DotenvzError::KeyNotFound { .. }));
        }

        #[test]
        #[ignore = "requires a running Secret Service daemon (gnome-keyring / KWallet)"]
        fn delete_missing_key_returns_key_not_found() {
            let err = p()
                .delete_secret(PROJECT, PROFILE, "LSS_GHOST_XYZ")
                .unwrap_err();
            assert!(matches!(err, DotenvzError::KeyNotFound { .. }));
        }

        #[test]
        #[ignore = "requires a running Secret Service daemon (gnome-keyring / KWallet)"]
        fn set_overwrites_existing_value() {
            let p = p();
            p.set_secret(PROJECT, PROFILE, "LSS_OVERWRITE", "old")
                .unwrap();
            p.set_secret(PROJECT, PROFILE, "LSS_OVERWRITE", "new")
                .unwrap();
            let v = p.get_secret(PROJECT, PROFILE, "LSS_OVERWRITE").unwrap();
            assert_eq!(v, "new");
            let _ = p.delete_secret(PROJECT, PROFILE, "LSS_OVERWRITE");
        }
    }
}
