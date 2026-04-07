# Provider Spec — macOS Providers

## Overview

dotenvz ships **two macOS providers** backed by the same Security.framework store:

| Provider key     | Storage target               | Default? |
|------------------|------------------------------|----------|
| `macos-passwords` | iCloud Keychain (synchronizable items; Passwords.app) | **Yes** (since dotenvz 0.2) |
| `macos-keychain`  | Local login Keychain only    | No (legacy / local-only) |

Both providers implement the full `SecretProvider` interface (`set`, `get`,
`list`, `delete`) and use the same service-namespace model and key registry
pattern. The only difference is `kSecAttrSynchronizable`.

---

## `macos-passwords` — iCloud Keychain / Passwords app

### Overview

`macos-passwords` is the **default macOS provider** written by `dotenvz init`.
Secrets are stored as **synchronizable** Generic Password items, making them:

- Visible in the macOS **Passwords** app (macOS 15+).
- Synced via **iCloud Keychain** across your signed-in Apple devices.

If iCloud Keychain is unavailable (iCloud signed out, `errSecMissingEntitlement`,
CI environment without an iCloud session, etc.) every operation silently falls
back to the **local login Keychain** using the identical service/account namespace.
No error is raised; callers cannot distinguish which store was used.

### Configuration

```toml
provider = "macos-passwords"
```

This is written automatically by `dotenvz init` on macOS.

### Secret storage layout

| Keychain attribute        | Value                          |
|---------------------------|--------------------------------|
| `kSecAttrService`         | `dotenvz.<project>.<profile>`  |
| `kSecAttrAccount`         | `<key>`                        |
| `kSecValueData`           | UTF-8 encoded `<value>`        |
| `kSecAttrSynchronizable`  | `kCFBooleanTrue`               |

On iCloud-unavailable fallback the item is written to the local Keychain **without**
`kSecAttrSynchronizable`. The key registry (see below) is likewise written to the
best available store.

### Operation flows

#### set_secret

```
dz set DATABASE_URL "postgres://localhost/mydb"
      │
      1. service = "dotenvz.<project>.<profile>"
      │
      2. sync_upsert(service, key, value)
      │     ├─ SecItemAdd({kSecAttrSynchronizable=true, …})
      │     ├─ on errSecDuplicateItem (-25299):
      │     │     SecItemUpdate(query, {kSecValueData=value})
      │     └─ on iCloud-unavailable (-25291, -34018, -25308):
      │           fall back to local upsert_password(service, key, value)
      │
      3. registry_add(service, key)
            └─ read_registry (sync first, then local)
            └─ append key if absent, write_registry (sync first, then local)
```

#### get_secret

```
dz get DATABASE_URL
      │
      1. service = "dotenvz.<project>.<profile>"
      │
      2. SecItemCopyMatching({kSecAttrSynchronizable=true, kSecReturnData=true})
      │     ├─ on errSecItemNotFound: continue to step 3
      │     └─ on iCloud-unavailable error: continue to step 3
      │
      3. (local fallback) get_generic_password(service, key)
      │     └─ on errSecItemNotFound: DotenvzError::KeyNotFound
      │
      4. UTF-8 decode bytes → return value string
```

#### list_secrets

```
dz list
      │
      1. service = "dotenvz.<project>.<profile>"
      │
      2. read_registry(service) → [KEY1, KEY2, …]
      │     (tries sync __dotenvz_idx__ first, then local)
      │
      3. for each key:
      │     get_secret → sync lookup then local fallback
      │       → KeyNotFound: skip (stale registry entry)
      │       → error: propagate
      │
      4. return HashMap<key, value>
```

#### delete_secret

```
dz rm DATABASE_URL
      │
      1. service = "dotenvz.<project>.<profile>"
      │
      2. SecItemDelete({kSecAttrSynchronizable=true})
      │     ├─ on errSecItemNotFound: note not found in iCloud (continue)
      │     └─ on iCloud-unavailable: note unavailable (continue)
      │
      3. delete_generic_password(service, key)   ← local arm
      │     └─ on errSecItemNotFound: note not found locally
      │
      4. if both not-found: DotenvzError::KeyNotFound
      │
      5. registry_remove(service, key)
```

### iCloud fallback error codes

The following Security.framework status codes are treated as "iCloud unavailable"
and trigger the local-Keychain fallback. All other non-zero codes are surfaced as
`DotenvzError::Provider`.

| Status code | Constant                   | Trigger scenario |
|-------------|----------------------------|------------------|
| -25291      | `errSecNotAvailable`       | iCloud Keychain service unavailable |
| -34018      | `errSecMissingEntitlement` | App lacks iCloud Keychain entitlement |
| -25308      | `errSecInteractionNotAllowed` | Headless / no UI context |

### Visibility in Passwords.app

Synchronizable items written by `macos-passwords` appear in **Settings → Passwords**
(macOS 15) or the **Passwords** app under:
- **Website:** `dotenvz.<project>.<profile>`
- **Username / Account:** the key name (e.g. `DATABASE_URL`)

The key registry sentinel (`__dotenvz_idx__`) is also visible. It should not be
deleted manually while dotenvz-managed secrets exist for that project/profile.

### Known limitation — iCloud entitlement required

> **`macos-passwords` currently always falls back to the local login Keychain
> when installed via `cargo install`.**

macOS requires the `com.apple.developer.icloud-keychain` entitlement to write
synchronizable Keychain items. Unsigned or ad-hoc-signed binaries receive
`errSecMissingEntitlement` (-34018) from Security.framework, which the provider
treats as an iCloud-unavailable condition and silently falls back to the local
login Keychain.

To use the real Passwords.app / iCloud sync path the binary must be:
1. Signed with a **Developer ID Application** certificate
2. Built with an entitlements plist containing `com.apple.developer.icloud-keychain = true`
3. **Notarized** by Apple for distribution outside the App Store

Until a signed release is available, `macos-passwords` and `macos-keychain` are
functionally identical for `cargo install`-built binaries. Secrets are stored
safely in the local login Keychain either way.

---

## `macos-keychain` — Local Login Keychain

### Overview

`macos-keychain` is the legacy provider. It stores secrets in the **local login
Keychain** only — no iCloud sync, no Passwords.app visibility.

Use this provider when:
- iCloud sync is explicitly undesired.
- You need full compatibility with secrets written by dotenvz 0.1.
- You run in an environment where iCloud is not available and you want no silent
  fallback ambiguity.

### Configuration

```toml
provider = "macos-keychain"
```

### Secret storage layout

| Keychain attribute  | Value                         |
|---------------------|-------------------------------|
| `kSecAttrService`   | `dotenvz.<project>.<profile>` |
| `kSecAttrAccount`   | `<key>`                       |
| `kSecValueData`     | UTF-8 encoded `<value>`       |

The service name format is: `dotenvz.<project>.<profile>`.

**Example:** project `my-app`, profile `dev`, key `DATABASE_URL`
→ service: `dotenvz.my-app.dev`, account: `DATABASE_URL`

---

## Key Registry (both providers)

macOS Keychain does not expose a native "list all accounts for a service" API
without CoreFoundation type-casting. To support `list_secrets` efficiently,
both macOS providers maintain a **key registry**: a newline-delimited list of
key names stored as a single Keychain item under the sentinel account
`__dotenvz_idx__` within the same service namespace.

| Keychain attribute  | Value                         |
|---------------------|-------------------------------|
| `kSecAttrService`   | `dotenvz.<project>.<profile>` |
| `kSecAttrAccount`   | `__dotenvz_idx__`             |
| `kSecValueData`     | newline-separated key names   |

For `macos-passwords`, the registry entry is also synchronizable. On fallback,
a local-keychain registry entry is maintained in parallel.

The registry is updated on every `set_secret` and `delete_secret` call:

- `set_secret` appends the key if not already present.
- `delete_secret` removes the key after the Keychain item is deleted.

**Registry drift:** If a Keychain item is deleted directly via the Keychain Access
UI (or `security` CLI), the registry will retain the stale entry. On the next
`list_secrets` call, stale keys produce a `KeyNotFound` result which are silently
skipped. The registry self-heals on the next `set_secret` or `delete_secret` for
the affected key.

---

## Operation Flows (`macos-keychain`)

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

## Authentication (both providers)

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

## Error Handling (both providers)

| Condition | `macos-passwords` | `macos-keychain` |
|-----------|-------------------|------------------|
| Key not found | `DotenvzError::KeyNotFound` (checks iCloud then local) | `DotenvzError::KeyNotFound` |
| iCloud unavailable | falls back to local silently | N/A |
| iCloud SecItem error (non-fallback code) | `DotenvzError::Provider("SecItem error (N)…")` | N/A |
| Keychain locked / access denied | `DotenvzError::Provider("…")` | `DotenvzError::Provider("…")` |
| Secret not valid UTF-8 | `DotenvzError::Provider("Invalid UTF-8…")` | `DotenvzError::Provider("Invalid UTF-8…")` |
| Upsert duplicate (handled) | transparent | transparent |
| Provider on non-macOS | `DotenvzError::UnsupportedPlatform` | `DotenvzError::UnsupportedPlatform` |

Secret values are never included in error messages.

---

## Key Name Constraints (both providers)

| Constraint                      | Detail                                                             |
|---------------------------------|--------------------------------------------------------------------|
| Encoding                        | Key names and values must be valid UTF-8.                          |
| Length                          | Effectively unlimited (Keychain supports long account strings).    |
| Reserved account name           | `__dotenvz_idx__` is used by the key registry; never use it as a key. |
| Characters                      | No restrictions; Keychain account strings accept arbitrary Unicode. |
| Case sensitivity                | Keys are case-sensitive (`foo` and `FOO` are separate items).      |

---

## Rust Crates

```toml
[target.'cfg(target_os = "macos")'.dependencies]
security-framework     = "2"   # high-level password helpers (macos-keychain)
security-framework-sys = "2"   # raw SecItem FFI (macos-passwords)
core-foundation        = "0.9" # CFDictionary, CFString, CFBoolean
```

---

## Visibility in Keychain Access / Passwords.app

### `macos-passwords`

Synchronizable items appear in:
- **Passwords.app** (macOS 15+) or **Settings → Passwords** under an entry whose
  website field is `dotenvz.<project>.<profile>` and username is the key name.
- **Keychain Access** (login keychain) as application passwords when the iCloud
  fallback path was used.

### `macos-keychain`

Items appear in **Keychain Access** under the login Keychain:

- **Kind:** application password
- **Where:** `dotenvz.<project>.<profile>`
- **Account:** the key name (e.g. `DATABASE_URL`)

For both providers, the key registry sentinel item (`__dotenvz_idx__`) is visible
with account name `__dotenvz_idx__`. It should not be deleted manually while
dotenvz-managed secrets exist for that project/profile.
