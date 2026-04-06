//! Windows Credential Manager provider.
//!
//! Stores secrets as Generic credentials in the Windows Credential Manager
//! using the Win32 `Cred*` API family.
//!
//! ## Secret storage layout
//!
//! | Credential attribute | Value                                          |
//! |----------------------|------------------------------------------------|
//! | Type                 | `CRED_TYPE_GENERIC`                            |
//! | TargetName           | `dotenvz/<project>/<profile>/<key>`            |
//! | CredentialBlob       | UTF-8 encoded secret value                     |
//! | Persist              | `CRED_PERSIST_LOCAL_MACHINE`                   |
//!
//! Because `CredEnumerateW` supports prefix wildcard filters natively
//! (`dotenvz/<project>/<profile>/*`), no key registry is required.

/// Secret provider backed by the Windows Credential Manager.
pub struct WindowsCredentialProvider;

impl WindowsCredentialProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WindowsCredentialProvider {
    fn default() -> Self {
        Self::new()
    }
}

// ── Windows implementation ──────────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod win_impl {
    use std::collections::HashMap;
    use std::ptr;

    use windows_sys::Win32::Security::Credentials::{
        CredDeleteW, CredEnumerateW, CredFree, CredReadW, CredWriteW, CREDENTIALW,
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    use crate::errors::{DotenvzError, Result};
    use crate::providers::secret_provider::SecretProvider;

    /// Windows error code: the item was not found.
    const ERROR_NOT_FOUND: u32 = 1168;

    fn to_wide_null(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0u16)).collect()
    }

    /// Return the last Win32 OS error code.
    fn last_os_error() -> u32 {
        std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32
    }

    /// Convert a null-terminated wide-char pointer to a `String`.
    ///
    /// # Safety
    /// `ptr` must point to a valid null-terminated UTF-16 sequence.
    unsafe fn wide_ptr_to_string(ptr: *const u16) -> String {
        if ptr.is_null() {
            return String::new();
        }
        let mut len = 0usize;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(ptr, len);
        String::from_utf16_lossy(slice)
    }

    impl SecretProvider for super::WindowsCredentialProvider {
        fn set_secret(&self, project: &str, profile: &str, key: &str, value: &str) -> Result<()> {
            let mut target = to_wide_null(&format!("dotenvz/{project}/{profile}/{key}"));
            let mut blob: Vec<u8> = value.as_bytes().to_vec();

            unsafe {
                let mut cred: CREDENTIALW = std::mem::zeroed();
                cred.Type = CRED_TYPE_GENERIC;
                cred.TargetName = target.as_mut_ptr();
                cred.CredentialBlobSize = blob.len() as u32;
                cred.CredentialBlob = blob.as_mut_ptr();
                cred.Persist = CRED_PERSIST_LOCAL_MACHINE;

                if CredWriteW(&cred, 0) == 0 {
                    let err = last_os_error();
                    return Err(DotenvzError::Provider(format!(
                        "CredWriteW failed (error {err})"
                    )));
                }
            }
            Ok(())
        }

        fn get_secret(&self, project: &str, profile: &str, key: &str) -> Result<String> {
            let target = to_wide_null(&format!("dotenvz/{project}/{profile}/{key}"));
            let mut cred_ptr: *mut CREDENTIALW = ptr::null_mut();

            unsafe {
                if CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut cred_ptr) == 0 {
                    let err = last_os_error();
                    if err == ERROR_NOT_FOUND {
                        return Err(DotenvzError::KeyNotFound {
                            key: key.to_string(),
                            profile: profile.to_string(),
                        });
                    }
                    return Err(DotenvzError::Provider(format!(
                        "CredReadW failed (error {err})"
                    )));
                }

                let cred = &*cred_ptr;
                let bytes = std::slice::from_raw_parts(
                    cred.CredentialBlob,
                    cred.CredentialBlobSize as usize,
                );
                let value = String::from_utf8_lossy(bytes).into_owned();
                CredFree(cred_ptr.cast());
                Ok(value)
            }
        }

        fn list_secrets(&self, project: &str, profile: &str) -> Result<HashMap<String, String>> {
            let filter = to_wide_null(&format!("dotenvz/{project}/{profile}/*"));
            let prefix = format!("dotenvz/{project}/{profile}/");
            let mut count: u32 = 0;
            let mut creds: *mut *mut CREDENTIALW = ptr::null_mut();

            unsafe {
                if CredEnumerateW(filter.as_ptr(), 0, &mut count, &mut creds) == 0 {
                    let err = last_os_error();
                    if err == ERROR_NOT_FOUND {
                        return Ok(HashMap::new());
                    }
                    return Err(DotenvzError::Provider(format!(
                        "CredEnumerateW failed (error {err})"
                    )));
                }

                let mut map = HashMap::new();
                let creds_slice = std::slice::from_raw_parts(creds, count as usize);
                for cred_ptr in creds_slice {
                    let cred = &**cred_ptr;
                    let target_name = wide_ptr_to_string(cred.TargetName);
                    if let Some(key) = target_name.strip_prefix(&prefix) {
                        let bytes = std::slice::from_raw_parts(
                            cred.CredentialBlob,
                            cred.CredentialBlobSize as usize,
                        );
                        let value = String::from_utf8_lossy(bytes).into_owned();
                        map.insert(key.to_string(), value);
                    }
                }
                CredFree(creds.cast());
                Ok(map)
            }
        }

        fn delete_secret(&self, project: &str, profile: &str, key: &str) -> Result<()> {
            let target = to_wide_null(&format!("dotenvz/{project}/{profile}/{key}"));

            unsafe {
                if CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) == 0 {
                    let err = last_os_error();
                    if err == ERROR_NOT_FOUND {
                        return Err(DotenvzError::KeyNotFound {
                            key: key.to_string(),
                            profile: profile.to_string(),
                        });
                    }
                    return Err(DotenvzError::Provider(format!(
                        "CredDeleteW failed (error {err})"
                    )));
                }
            }
            Ok(())
        }
    }
}

// ── Non-Windows stub ────────────────────────────────────────────────────────

#[cfg(not(target_os = "windows"))]
impl crate::providers::secret_provider::SecretProvider for WindowsCredentialProvider {
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
