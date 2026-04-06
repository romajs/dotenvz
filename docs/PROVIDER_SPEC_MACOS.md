# Provider Spec — macOS Keychain

## Overview

`macos-keychain` is the default dotenvz provider on macOS. It stores, retrieves,
and deletes secrets using the **macOS Security.framework Generic Password API** —
the same underlying store used by Safari, Xcode, and system services.

All operations are fully supported (set, get, list, delete). Secrets never leave
the local machine and require no cloud account or network access.

---

## Configuration

Declare the provider under `[providers.<name>]` in `.dotenvz.toml`.
The provider name is a local alias; the `type` field identifies the backend.

```toml
[providers.local]
type = "macos-keychain"
```

### Config fields

| Field  | Type   | Required | Description                      |
|--------|--------|----------|----------------------------------|
| `type` | string | yes      | Must be `"macos-keychain"`.      |

There are no additional configuration fields. The Keychain service namespace is
derived automatically from the `project` and `profile` values in the active config.

### Activating the provider for a profile

```toml
[profiles.dev]
provider = "local"

[profiles.staging]
provider = "local"
```

Since there is only one Keychain per user login, all profiles on the same machine
share the same physical store. Secrets are isolated by the service namespace
`dotenvz.<project>.<profile>`, so a key named `DATABASE_URL` in `dev` and the same
key in `staging` are stored as separate Keychain items and never conflict.

---

## Secret Storage Layout

Each secret is stored as a **Generic Password** item in the macOS login Keychain:

| Keychain attribute  | Value                         |
|---------------------|-------------------------------|
| `kSecAttrService`   | `dotenvz.<project>.<profile>` |
| `kSecAttrAccount`   | `<key>`                       |
| `kSecValueData`     | UTF-8 encoded `<value>`       |

The service name format is: `dotenvz.<project>.<profile>`.

**Example:** project `my-app`, profile `dev`, key `DATABASE_URL`
→ service: `dotenvz.my-app.dev`, account: `DATABASE_URL`

---

## Key Registry

macOS Keychain does not expose a native "list all accounts for a service" API
without CoreFoundation type-casting. To support `list_secrets` efficiently,
the provider maintains a **key registry**: a newline-delimited list of key names
stored as a single Keychain item under the sentinel account `__dotenvz_idx__`
within the same service namespace.

| Keychain attribute  | Value                         |
|---------------------|-------------------------------|
| `kSecAttrService`   | `dotenvz.<project>.<profile>` |
| `kSecAttrAccount`   | `__dotenvz_idx__`             |
| `kSecValueData`     | newline-separated key names   |

The registry is updated on every `set_secret` and `delete_secret` call:

- `set_secret` appends the key if not already present.
- `delete_secret` removes the key after the Keychain item is deleted.

**Registry drift:** If a Keychain item is deleted directly via the Keychain Access
UI (or `security` CLI), the registry will retain the stale entry. On the next
`list_secrets` call, stale keys produce a `KeyNotFound` result which are silently
skipped. The registry self-heals on the next `set_secret` or `delete_secret` for
the affected key.

---

## Operation Flow

### set_secret

```
dz set DATABASE_URL "postgres://localhost/mydb"
      │
      1. service = "dotenvz.<project>.<profile>"
      │
      2. upsert_password(service, key, value)
      │     └─ set_generic_password(service, key, value)
      │           → on errSecDuplicateItem (-25299):
      │                 delete_generic_password(service, key)
      │                 set_generic_password(service, key, value)
      │
      3. registry_add(service, key)
            └─ read __dotenvz_idx__, append key if absent, write back
```

### get_secret

```
dz get DATABASE_URL
      │
      1. service = "dotenvz.<project>.<profile>"
      │
      2. get_generic_password(service, key)
      │     → on errSecItemNotFound (-25300): DotenvzError::KeyNotFound
      │     → other error: DotenvzError::Provider
      │
      3. UTF-8 decode bytes → return value string
```

### list_secrets

```
dz list
      │
      1. service = "dotenvz.<project>.<profile>"
      │
      2. read_registry(service) → [KEY1, KEY2, …]
      │
      3. for each key:
      │     get_generic_password(service, key)
      │       → KeyNotFound: skip (stale registry entry)
      │       → error: propagate
      │
      4. return HashMap<key, value>
```

### delete_secret

```
dz rm DATABASE_URL
      │
      1. service = "dotenvz.<project>.<profile>"
      │
      2. delete_generic_password(service, key)
      │     → on errSecItemNotFound: DotenvzError::KeyNotFound
      │
      3. registry_remove(service, key)
            └─ read __dotenvz_idx__, filter out key, write back
```

---

## Authentication

No authentication configuration is required. Access is controlled by the macOS
user session. The Keychain is unlocked automatically for the logged-in user.

dotenvz does not:
- Store or manage Keychain passwords.
- Prompt for the login Keychain password (macOS handles this at the OS level).
- Support custom Keychain files or non-login Keychains.

**Keychain lock behaviour:** If the login Keychain is locked (e.g. after a screen
lock with "lock Keychain after screen lock" enabled), macOS may display a system
dialog asking for the Keychain password before granting access. This dialog is
outside dotenvz's control.

---

## Error Handling

| Condition                              | dotenvz error                  | Notes                                                         |
|----------------------------------------|--------------------------------|---------------------------------------------------------------|
| Key does not exist                     | `DotenvzError::KeyNotFound`    | `errSecItemNotFound` (-25300) from Security.framework         |
| Keychain is locked / access denied     | `DotenvzError::Provider("…")` | OS may display an unlock dialog; error returned if dismissed  |
| Secret value is not valid UTF-8        | `DotenvzError::Provider("…")` | Binary data stored by another app under the same account      |
| Keychain item already exists (upsert)  | handled internally             | Delete-then-rewrite; never surfaced to the caller             |
| Registry read returns invalid UTF-8    | returns empty key list         | Treated as empty registry; next write reconciles              |
| Provider called on non-macOS platform  | `DotenvzError::UnsupportedPlatform` | Stub implementation returns this on Linux / Windows      |

All `DotenvzError::Provider` payloads include the original Security.framework OSStatus
code. Secret values are never included in error messages.

---

## Key Name Constraints

| Constraint                      | Detail                                                             |
|---------------------------------|--------------------------------------------------------------------|
| Encoding                        | Key names and values must be valid UTF-8.                          |
| Length                          | Effectively unlimited (Keychain supports long account strings).    |
| Reserved account name           | `__dotenvz_idx__` is used by the key registry; never use it as a key. |
| Characters                      | No restrictions; Keychain account strings accept arbitrary Unicode. |
| Case sensitivity                | Keys are case-sensitive (`foo` and `FOO` are separate items).      |

---

## Rust Crate

```toml
[target.'cfg(target_os = "macos")'.dependencies]
security-framework = "2"
core-foundation     = "0.9"
```

The `security-framework` crate provides safe Rust bindings for the macOS
Security.framework C API. No system libraries need to be installed separately;
Security.framework ships with every macOS installation.

---

## Visibility in Keychain Access

Secrets stored by dotenvz are visible in the **Keychain Access** application
(Applications → Utilities → Keychain Access) under the login Keychain. Items appear
with:

- **Kind:** application password
- **Where:** `dotenvz.<project>.<profile>`
- **Account:** the key name (e.g. `DATABASE_URL`)

The key registry sentinel item (`__dotenvz_idx__`) is also visible in Keychain Access
with the account name `__dotenvz_idx__`. It should not be deleted manually while
dotenvz-managed secrets exist for that project/profile.
