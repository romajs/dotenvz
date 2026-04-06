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
