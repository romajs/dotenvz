# Provider Spec — Windows Credential Manager

## Overview

`windows-credential` is the default dotenvz provider on Windows. It stores,
retrieves, and deletes secrets using the **Win32 Credential API** (`Cred*` family) —
the same underlying store used by Windows itself for browser passwords, network
credentials, and application secrets.

All operations are fully supported (set, get, list, delete). Secrets are stored
locally on the machine in the Windows Credential Manager vault. No cloud account
or network access is required.

---

## Configuration

```toml
[providers.local]
type = "windows-credential"
```

### Config fields

| Field  | Type   | Required | Description                           |
|--------|--------|----------|---------------------------------------|
| `type` | string | yes      | Must be `"windows-credential"`.       |

There are no additional configuration fields. The credential `TargetName` namespace
is derived automatically from the `project` and `profile` values in the active config.

### Activating the provider for a profile

```toml
[profiles.dev]
provider = "local"

[profiles.staging]
provider = "local"
```

All profiles on the same machine share the same Credential Manager vault. Secrets
are isolated by `TargetName` prefix `dotenvz/<project>/<profile>/`, so
`DATABASE_URL` in `dev` and `DATABASE_URL` in `staging` are distinct credentials
and never conflict.

---

## Secret Storage Layout

Each secret is stored as a **Generic** credential (`CRED_TYPE_GENERIC`) in the
Windows Credential Manager:

| Credential attribute | Value                               |
|----------------------|-------------------------------------|
| `Type`               | `CRED_TYPE_GENERIC`                 |
| `TargetName`         | `dotenvz/<project>/<profile>/<key>` |
| `CredentialBlob`     | UTF-8 encoded secret value          |
| `Persist`            | `CRED_PERSIST_LOCAL_MACHINE`        |
| `UserName`           | not set (empty)                     |

The `TargetName` format is: `dotenvz/<project>/<profile>/<key>`.

**Example:** project `my-app`, profile `dev`, key `DATABASE_URL`
→ `TargetName`: `dotenvz/my-app/dev/DATABASE_URL`

`CRED_PERSIST_LOCAL_MACHINE` means credentials are stored in the machine's
protected storage and survive user logoff. They are bound to the Windows user
account and are not accessible by other user accounts on the same machine.

---

## Key Enumeration

Windows Credential Manager supports **prefix wildcard filters** natively via
`CredEnumerateW`. `list_secrets` passes the filter
`dotenvz/<project>/<profile>/*` to enumerate all credentials for a specific
project and profile in a single API call.

No separate key registry is required. Credentials deleted directly through the
Credential Manager UI are automatically absent from the next `list_secrets` result.

---

## Operation Flow

### set_secret

```
dz set DATABASE_URL "postgres://localhost/mydb"
      │
      1. target = to_wide_null("dotenvz/<project>/<profile>/<key>")
      │
      2. Build CREDENTIALW {
      │     Type             = CRED_TYPE_GENERIC,
      │     TargetName       = target,
      │     CredentialBlob   = value.as_bytes(),
      │     CredentialBlobSize = value.len() as u32,
      │     Persist          = CRED_PERSIST_LOCAL_MACHINE,
      │   }
      │
      3. CredWriteW(&cred, 0)
            → if BOOL == 0: DotenvzError::Provider("CredWriteW failed (error N)")
            ← CredWriteW automatically overwrites existing credentials with the
               same TargetName; no duplicate-item handling needed.
```

### get_secret

```
dz get DATABASE_URL
      │
      1. target = to_wide_null("dotenvz/<project>/<profile>/<key>")
      │
      2. CredReadW(target, CRED_TYPE_GENERIC, 0, &mut cred_ptr)
      │     → BOOL == 0 && error == ERROR_NOT_FOUND (1168):
      │           DotenvzError::KeyNotFound
      │     → BOOL == 0 && other error:
      │           DotenvzError::Provider("CredReadW failed (error N)")
      │
      3. bytes = slice_from_raw_parts(cred.CredentialBlob, cred.CredentialBlobSize)
      │
      4. String::from_utf8_lossy(bytes) → return value string
      │
      5. CredFree(cred_ptr)
```

### list_secrets

```
dz list
      │
      1. filter = to_wide_null("dotenvz/<project>/<profile>/*")
      │
      2. CredEnumerateW(filter, 0, &mut count, &mut creds)
      │     → BOOL == 0 && error == ERROR_NOT_FOUND: return empty HashMap
      │     → BOOL == 0 && other error: DotenvzError::Provider
      │
      3. for each credential in slice_from_raw_parts(creds, count):
      │     target_name = wide_ptr_to_string(cred.TargetName)
      │     key = target_name.strip_prefix("dotenvz/<project>/<profile>/")
      │     value = String::from_utf8_lossy(cred.CredentialBlob)
      │     map.insert(key, value)
      │
      4. CredFree(creds)
      │
      5. return HashMap<key, value>
```

### delete_secret

```
dz rm DATABASE_URL
      │
      1. target = to_wide_null("dotenvz/<project>/<profile>/<key>")
      │
      2. CredDeleteW(target, CRED_TYPE_GENERIC, 0)
            → BOOL == 0 && error == ERROR_NOT_FOUND (1168):
                  DotenvzError::KeyNotFound
            → BOOL == 0 && other error:
                  DotenvzError::Provider("CredDeleteW failed (error N)")
```

---

## Authentication

No authentication configuration is required. Access is controlled by the Windows
user account. The Credential Manager vault is unlocked automatically when the user
is logged in.

dotenvz does not:
- Prompt for Windows credentials or vault passwords.
- Support DPAPI (Data Protection API) key management directly.
- Access credentials belonging to other user accounts or the `CRED_PERSIST_ENTERPRISE`
  or `CRED_PERSIST_SESSION` stores.

**UAC / elevated processes:** Credentials stored by a non-elevated process are
accessible to elevated processes running under the same user account, and vice versa.
No special elevation is needed to use dotenvz.

---

## Error Handling

| Condition                              | dotenvz error                        | Notes                                                          |
|----------------------------------------|--------------------------------------|----------------------------------------------------------------|
| Key does not exist                     | `DotenvzError::KeyNotFound`          | `ERROR_NOT_FOUND` (1168) from `CredReadW` or `CredDeleteW`     |
| `CredWriteW` failure                   | `DotenvzError::Provider("…")`       | Includes Win32 error code in message                           |
| `CredReadW` failure (non-404)          | `DotenvzError::Provider("…")`       | Includes Win32 error code in message                           |
| `CredEnumerateW` failure (non-404)     | `DotenvzError::Provider("…")`       | Includes Win32 error code in message                           |
| `CredDeleteW` failure (non-404)        | `DotenvzError::Provider("…")`       | Includes Win32 error code in message                           |
| Secret value exceeds 2560 bytes        | `DotenvzError::Provider("…")`       | OS-level `ERROR_BAD_LENGTH`; see size constraint below         |
| Provider called on non-Windows platform | `DotenvzError::UnsupportedPlatform` | Stub returns this on macOS / Linux                             |

All `DotenvzError::Provider` payloads include the raw Win32 error code returned by
`GetLastError()`. Secret values are never included in error messages.

---

## Key Name Constraints

| Constraint           | Detail                                                                                    |
|----------------------|-------------------------------------------------------------------------------------------|
| Encoding             | Key names and values must be valid UTF-8. `TargetName` is converted to UTF-16 internally. |
| `TargetName` length  | Maximum 512 characters for `CRED_TYPE_GENERIC` (Win32 limit: `CRED_MAX_GENERIC_TARGET_NAME_LENGTH`). |
| Value size           | Maximum **2560 bytes** (`CRED_MAX_CREDENTIAL_BLOB_SIZE`). Larger values fail at the OS level with `ERROR_BAD_LENGTH`. |
| Characters           | No restrictions on key characters; slashes (`/`) in the key are allowed and not treated as separators by dotenvz. |
| Case sensitivity     | `TargetName` comparisons in Credential Manager are **case-insensitive**. Two keys that differ only in case will map to the same credential. |
| Reserved prefix      | `dotenvz/` is used as the TargetName prefix; do not use keys that when prefixed produce a name matching an unrelated credential. |

---

## Rust Crate

```toml
[target.'cfg(target_os = "windows")'.dependencies]
windows-sys = { version = "0.61", features = [
    "Win32_Security_Credentials",
    "Win32_Foundation",
] }
```

The `windows-sys` crate provides raw FFI bindings to the Win32 API. The
`Win32_Security_Credentials` feature exposes the `Cred*` functions and
`CREDENTIALW` structs. The `Win32_Foundation` feature provides `BOOL` and
related primitive types.

All Win32 calls in the provider use `unsafe` blocks with explicit null-pointer
checks and `GetLastError()` inspection before constructing `DotenvzError` values.

---

## Visibility in Credential Manager

Secrets stored by dotenvz are visible in the **Windows Credential Manager** UI:

- **Control Panel → Credential Manager → Windows Credentials**
- Each entry appears under the **Generic Credentials** section.
- **Internet or network address** column shows: `dotenvz/<project>/<profile>/<key>`
- **Type** shows: Generic

Credentials can be viewed, edited, or deleted through this UI. Changes made through
the UI are immediately reflected in subsequent dotenvz calls (no cache to invalidate).

> **Note:** The Credential Manager UI shows the `CredentialBlob` as a masked
> password field. An authorized user with access to the Windows account can reveal
> the stored value through the UI. Access control is entirely delegated to the
> Windows user account boundary.
