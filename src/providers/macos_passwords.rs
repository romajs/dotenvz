//! macOS Passwords provider.
//!
//! Stores secrets as **synchronizable** Generic Password items so they appear
//! in the macOS Passwords app (iCloud Keychain) on macOS 15 and later.
//!
//! ## Storage layout
//!
//! Identical to `macos_keychain`, with one extra attribute:
//!
//! | Keychain attribute        | Value                          |
//! |---------------------------|--------------------------------|
//! | `kSecAttrService`         | `dotenvz.<project>.<profile>`  |
//! | `kSecAttrAccount`         | `<key>`                        |
//! | `kSecValueData`           | UTF-8 encoded `<value>`        |
//! | `kSecAttrSynchronizable`  | `kCFBooleanTrue`               |
//!
//! ## Fallback behaviour
//!
//! If iCloud Keychain is unavailable the provider transparently falls back to
//! the local login Keychain. Fallback is triggered on:
//! - `errSecNotAvailable` (-25291)
//! - `errSecMissingEntitlement` (-34018)
//! - `errSecInteractionNotAllowed` (-25308)

use std::collections::HashMap;

use crate::errors::{DotenvzError, Result};
use crate::providers::secret_provider::SecretProvider;

/// Secret provider that prefers iCloud Keychain (Passwords.app) with a
/// transparent fallback to the local login Keychain.
pub struct MacOsPasswordsProvider;

impl MacOsPasswordsProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MacOsPasswordsProvider {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// macOS implementation
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use core_foundation::{
        base::{CFType, TCFType},
        boolean::CFBoolean,
        data::CFData,
        dictionary::CFDictionary,
        string::CFString,
    };
    use security_framework_sys::{
        base::errSecSuccess,
        item::{
            kSecAttrAccount, kSecAttrService, kSecAttrSynchronizable, kSecClass,
            kSecClassGenericPassword, kSecReturnData, kSecValueData,
        },
        keychain_item::{SecItemAdd, SecItemCopyMatching, SecItemDelete, SecItemUpdate},
    };

    /// Account name used as the key registry sentinel.
    pub(super) const REGISTRY_ACCOUNT: &str = "__dotenvz_idx__";

    /// `errSecItemNotFound` (-25300)
    const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
    /// `errSecDuplicateItem` (-25299)
    const ERR_SEC_DUPLICATE_ITEM: i32 = -25299;

    /// Status codes that indicate iCloud sync is unavailable.
    const ICLOUD_UNAVAILABLE: &[i32] = &[
        -25291, // errSecNotAvailable
        -34018, // errSecMissingEntitlement
        -25308, // errSecInteractionNotAllowed
    ];

    /// Build the Keychain service name.
    pub(super) fn service_name(project: &str, profile: &str) -> String {
        format!("dotenvz.{project}.{profile}")
    }

    fn is_icloud_unavailable(code: i32) -> bool {
        ICLOUD_UNAVAILABLE.contains(&code)
    }

    // ─── Low-level synchronizable SecItem helpers ─────────────────────────

    /// Build the base query dict (service + account + synchronizable=true).
    ///
    /// Omitting `kSecMatchLimit` relies on the Security framework default which
    /// returns at most one item — identical to `kSecMatchLimitOne`.
    fn base_query(service: &str, account: &str) -> CFDictionary<CFString, CFType> {
        let svc = CFString::new(service);
        let acct = CFString::new(account);
        let sync = CFBoolean::true_value();
        unsafe {
            CFDictionary::from_CFType_pairs(&[
                (
                    CFString::wrap_under_get_rule(kSecClass),
                    CFString::wrap_under_get_rule(kSecClassGenericPassword).as_CFType(),
                ),
                (
                    CFString::wrap_under_get_rule(kSecAttrService),
                    svc.as_CFType(),
                ),
                (
                    CFString::wrap_under_get_rule(kSecAttrAccount),
                    acct.as_CFType(),
                ),
                (
                    CFString::wrap_under_get_rule(kSecAttrSynchronizable),
                    sync.as_CFType(),
                ),
            ])
        }
    }

    /// Add a synchronizable generic password item.
    fn secitem_add_sync(service: &str, account: &str, data: &[u8]) -> std::result::Result<(), i32> {
        let svc = CFString::new(service);
        let acct = CFString::new(account);
        let val = CFData::from_buffer(data);
        let sync = CFBoolean::true_value();
        let dict: CFDictionary<CFString, CFType> = unsafe {
            CFDictionary::from_CFType_pairs(&[
                (
                    CFString::wrap_under_get_rule(kSecClass),
                    CFString::wrap_under_get_rule(kSecClassGenericPassword).as_CFType(),
                ),
                (
                    CFString::wrap_under_get_rule(kSecAttrService),
                    svc.as_CFType(),
                ),
                (
                    CFString::wrap_under_get_rule(kSecAttrAccount),
                    acct.as_CFType(),
                ),
                (
                    CFString::wrap_under_get_rule(kSecAttrSynchronizable),
                    sync.as_CFType(),
                ),
                (
                    CFString::wrap_under_get_rule(kSecValueData),
                    val.as_CFType(),
                ),
            ])
        };
        let status = unsafe { SecItemAdd(dict.as_concrete_TypeRef(), std::ptr::null_mut()) };
        if status == errSecSuccess {
            Ok(())
        } else {
            Err(status)
        }
    }

    /// Update an existing synchronizable generic password's data.
    fn secitem_update_sync(
        service: &str,
        account: &str,
        data: &[u8],
    ) -> std::result::Result<(), i32> {
        let val = CFData::from_buffer(data);
        let query = base_query(service, account);
        let attrs: CFDictionary<CFString, CFType> = unsafe {
            CFDictionary::from_CFType_pairs(&[(
                CFString::wrap_under_get_rule(kSecValueData),
                val.as_CFType(),
            )])
        };
        let status =
            unsafe { SecItemUpdate(query.as_concrete_TypeRef(), attrs.as_concrete_TypeRef()) };
        if status == errSecSuccess {
            Ok(())
        } else {
            Err(status)
        }
    }

    /// Set a synchronizable password, updating if it already exists.
    fn sync_upsert(service: &str, account: &str, data: &[u8]) -> std::result::Result<(), i32> {
        match secitem_add_sync(service, account, data) {
            Ok(()) => Ok(()),
            Err(ERR_SEC_DUPLICATE_ITEM) => secitem_update_sync(service, account, data),
            Err(e) => Err(e),
        }
    }

    /// Fetch a synchronizable generic password's data bytes.
    fn sync_get(service: &str, account: &str) -> std::result::Result<Vec<u8>, i32> {
        let ret_data = CFBoolean::true_value();
        let query = base_query(service, account);

        // Extend the query dict with kSecReturnData = true.
        // Build a fresh dict including kSecReturnData.
        let svc = CFString::new(service);
        let acct = CFString::new(account);
        let sync = CFBoolean::true_value();
        let dict: CFDictionary<CFString, CFType> = unsafe {
            CFDictionary::from_CFType_pairs(&[
                (
                    CFString::wrap_under_get_rule(kSecClass),
                    CFString::wrap_under_get_rule(kSecClassGenericPassword).as_CFType(),
                ),
                (
                    CFString::wrap_under_get_rule(kSecAttrService),
                    svc.as_CFType(),
                ),
                (
                    CFString::wrap_under_get_rule(kSecAttrAccount),
                    acct.as_CFType(),
                ),
                (
                    CFString::wrap_under_get_rule(kSecAttrSynchronizable),
                    sync.as_CFType(),
                ),
                (
                    CFString::wrap_under_get_rule(kSecReturnData),
                    ret_data.as_CFType(),
                ),
            ])
        };
        // Suppress the "unused variable" warning for the intermediate.
        let _ = query;

        let mut result: core_foundation::base::CFTypeRef = std::ptr::null();
        let status = unsafe { SecItemCopyMatching(dict.as_concrete_TypeRef(), &mut result) };
        if status == errSecSuccess {
            // result is a CFData (kSecReturnData=true, single-item default).
            let data = unsafe {
                CFData::wrap_under_create_rule(std::mem::transmute::<
                    *const std::ffi::c_void,
                    *const core_foundation::data::__CFData,
                >(result))
            };
            Ok(data.bytes().to_vec())
        } else {
            Err(status)
        }
    }

    /// Delete a synchronizable generic password.
    /// Returns `Err(ERR_SEC_ITEM_NOT_FOUND)` when the item does not exist so
    /// callers can distinguish "deleted" from "was not there".
    fn sync_delete(service: &str, account: &str) -> std::result::Result<(), i32> {
        let query = base_query(service, account);
        let status = unsafe { SecItemDelete(query.as_concrete_TypeRef()) };
        if status == errSecSuccess {
            Ok(())
        } else {
            Err(status)
        }
    }

    // ─── Registry helpers ─────────────────────────────────────────────────

    fn parse_registry(bytes: Vec<u8>) -> Vec<String> {
        String::from_utf8(bytes)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect()
    }

    /// Read the key registry, trying iCloud first then local.
    pub(super) fn read_registry(service: &str) -> Vec<String> {
        if let Ok(bytes) = sync_get(service, REGISTRY_ACCOUNT) {
            return parse_registry(bytes);
        }
        match security_framework::passwords::get_generic_password(service, REGISTRY_ACCOUNT) {
            Ok(bytes) => parse_registry(bytes),
            Err(_) => vec![],
        }
    }

    fn write_registry(service: &str, keys: &[String]) -> Result<()> {
        let data = keys.join("\n");
        upsert_password_sync(service, REGISTRY_ACCOUNT, data.as_bytes())
    }

    pub(super) fn registry_add(service: &str, key: &str) -> Result<()> {
        let mut keys = read_registry(service);
        if !keys.iter().any(|k| k == key) {
            keys.push(key.to_owned());
            write_registry(service, &keys)?;
        }
        Ok(())
    }

    pub(super) fn registry_remove(service: &str, key: &str) -> Result<()> {
        let keys: Vec<String> = read_registry(service)
            .into_iter()
            .filter(|k| k != key)
            .collect();
        write_registry(service, &keys)
    }

    // ─── High-level helpers with fallback ─────────────────────────────────

    /// Upsert using iCloud sync; fall back to local keychain on iCloud error.
    pub(super) fn upsert_password_sync(service: &str, account: &str, data: &[u8]) -> Result<()> {
        match sync_upsert(service, account, data) {
            Ok(()) => Ok(()),
            Err(code) if is_icloud_unavailable(code) => {
                upsert_password_local(service, account, data)
            }
            Err(code) => Err(DotenvzError::Provider(format!(
                "SecItem error ({code}) writing to iCloud Keychain"
            ))),
        }
    }

    fn upsert_password_local(service: &str, account: &str, data: &[u8]) -> Result<()> {
        use security_framework::passwords::{delete_generic_password, set_generic_password};
        const ERR_DUPLICATE: i32 = -25299;
        match set_generic_password(service, account, data) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == ERR_DUPLICATE => {
                let _ = delete_generic_password(service, account);
                set_generic_password(service, account, data)
                    .map_err(|e| DotenvzError::Provider(e.to_string()))
            }
            Err(e) => Err(DotenvzError::Provider(e.to_string())),
        }
    }

    /// Get a password, trying iCloud then local keychain.
    pub(super) fn get_password_with_fallback(
        service: &str,
        account: &str,
        key: &str,
        profile: &str,
    ) -> Result<String> {
        match sync_get(service, account) {
            Ok(bytes) => {
                return String::from_utf8(bytes).map_err(|e| {
                    DotenvzError::Provider(format!("Invalid UTF-8 in secret `{key}`: {e}"))
                });
            }
            Err(ERR_SEC_ITEM_NOT_FOUND) => {}
            Err(code) if is_icloud_unavailable(code) => {}
            Err(code) => {
                return Err(DotenvzError::Provider(format!(
                    "SecItem error ({code}) reading from iCloud Keychain"
                )));
            }
        }

        security_framework::passwords::get_generic_password(service, account)
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
                String::from_utf8(bytes).map_err(|e| {
                    DotenvzError::Provider(format!("Invalid UTF-8 in secret `{key}`: {e}"))
                })
            })
    }

    /// Delete from both iCloud and local; error only if neither contained the key.
    pub(super) fn delete_with_fallback(
        service: &str,
        account: &str,
        key: &str,
        profile: &str,
    ) -> Result<()> {
        let sync_found = match sync_delete(service, account) {
            Ok(()) => true,
            Err(ERR_SEC_ITEM_NOT_FOUND) => false,
            Err(code) if is_icloud_unavailable(code) => false,
            Err(code) => {
                return Err(DotenvzError::Provider(format!(
                    "SecItem error ({code}) deleting from iCloud Keychain"
                )))
            }
        };

        let local_found =
            match security_framework::passwords::delete_generic_password(service, account) {
                Ok(()) => true,
                Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => false,
                Err(e) => return Err(DotenvzError::Provider(e.to_string())),
            };

        if sync_found || local_found {
            Ok(())
        } else {
            Err(DotenvzError::KeyNotFound {
                key: key.to_string(),
                profile: profile.to_string(),
            })
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SecretProvider — non-macOS stub
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "macos"))]
impl SecretProvider for MacOsPasswordsProvider {
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

// ─────────────────────────────────────────────────────────────────────────────
// SecretProvider — macOS implementation
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
impl SecretProvider for MacOsPasswordsProvider {
    fn set_secret(&self, project: &str, profile: &str, key: &str, value: &str) -> Result<()> {
        let service = imp::service_name(project, profile);
        imp::upsert_password_sync(&service, key, value.as_bytes())?;
        imp::registry_add(&service, key)
    }

    fn get_secret(&self, project: &str, profile: &str, key: &str) -> Result<String> {
        let service = imp::service_name(project, profile);
        imp::get_password_with_fallback(&service, key, key, profile)
    }

    fn list_secrets(&self, project: &str, profile: &str) -> Result<HashMap<String, String>> {
        let service = imp::service_name(project, profile);
        let keys = imp::read_registry(&service);

        let mut map = HashMap::new();
        for key in keys {
            if key == imp::REGISTRY_ACCOUNT {
                continue;
            }
            match self.get_secret(project, profile, &key) {
                Ok(value) => {
                    map.insert(key, value);
                }
                Err(DotenvzError::KeyNotFound { .. }) => {
                    // Stale registry entry — skip silently.
                }
                Err(e) => return Err(e),
            }
        }
        Ok(map)
    }

    fn delete_secret(&self, project: &str, profile: &str, key: &str) -> Result<()> {
        let service = imp::service_name(project, profile);
        imp::delete_with_fallback(&service, key, key, profile)?;
        imp::registry_remove(&service, key)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_constructible() {
        let _p = MacOsPasswordsProvider::new();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn service_name_format() {
        assert_eq!(imp::service_name("my-app", "dev"), "dotenvz.my-app.dev");
        assert_eq!(imp::service_name("proj", "prod"), "dotenvz.proj.prod");
        assert_eq!(imp::service_name("a", "b"), "dotenvz.a.b");
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn non_macos_set_returns_unsupported() {
        let p = MacOsPasswordsProvider::new();
        let err = p.set_secret("proj", "dev", "KEY", "val").unwrap_err();
        assert!(matches!(err, DotenvzError::UnsupportedPlatform));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn non_macos_list_returns_unsupported() {
        let p = MacOsPasswordsProvider::new();
        let err = p.list_secrets("proj", "dev").unwrap_err();
        assert!(matches!(err, DotenvzError::UnsupportedPlatform));
    }
}
