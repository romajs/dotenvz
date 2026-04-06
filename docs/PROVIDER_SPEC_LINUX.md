# Provider Spec — Linux Secret Service

## Overview

`linux-secret-service` is the default dotenvz provider on Linux. It stores,
retrieves, and deletes secrets using the **Secret Service D-Bus API** — the
standard interface implemented by **GNOME Keyring** and **KWallet**, the two most
common desktop keyring daemons on Linux.

All operations are fully supported (set, get, list, delete). Secrets are stored
locally on the machine inside the active keyring daemon's encrypted storage.
No cloud account or network access is required.

---

## Configuration

```toml
[providers.local]
type = "linux-secret-service"
```

### Config fields

| Field  | Type   | Required | Description                              |
|--------|--------|----------|------------------------------------------|
| `type` | string | yes      | Must be `"linux-secret-service"`.        |

There are no additional configuration fields. The keyring collection used is the
daemon's **default collection** (typically `"login"` in GNOME Keyring and
`"kdewallet"` in KWallet). The project and profile are stored as D-Bus item
attributes, not embedded in the collection name.

### Activating the provider for a profile

```toml
[profiles.dev]
provider = "local"

[profiles.staging]
provider = "local"
```

All profiles on the same machine share the same default collection. Secrets are
isolated by item attributes `(application="dotenvz", project, profile)`, so
`DATABASE_URL` in `dev` and `DATABASE_URL` in `staging` are distinct items and
never conflict.

---

## Secret Storage Layout

Each secret is stored as an **item** in the default Secret Service collection,
carrying the following D-Bus attributes:

| Attribute     | Value                    |
|---------------|--------------------------|
| `application` | `"dotenvz"` (constant)   |
| `project`     | project name from config |
| `profile`     | active profile name      |
| `key`         | env-var key name         |

The item **label** (human-readable display name) is:

```
dotenvz/<project>/<profile>/<key>
```

The item **secret** is the UTF-8 encoded value string, with content type
`"text/plain"`.

**Example:** project `my-app`, profile `dev`, key `DATABASE_URL`
→ label: `dotenvz/my-app/dev/DATABASE_URL`
→ attributes: `{ application: "dotenvz", project: "my-app", profile: "dev", key: "DATABASE_URL" }`

---

## Key Enumeration

Because Secret Service supports native attribute-based search, **no separate key
registry is required**. `list_secrets` queries the collection with the partial
attribute set `(application, project, profile)` and receives all matching items in
a single D-Bus call. Each item's `key` attribute is extracted to build the result map.

This is simpler and more reliable than the macOS registry approach: items deleted
directly through the keyring UI are automatically absent from the next
`list_secrets` result.

---

## Operation Flow

### set_secret

```
dz set DATABASE_URL "postgres://localhost/mydb"
      │
      1. SecretService::connect(EncryptionType::Dh)
      │
      2. collection = ss.get_default_collection()
      │     └─ collection.ensure_unlocked()
      │
      3. attrs = { application:"dotenvz", project, profile, key }
      │
      4. collection.create_item(
      │     label    = "dotenvz/<project>/<profile>/<key>",
      │     attrs    = attrs,
      │     secret   = value.as_bytes(),
      │     replace  = true,           ← overwrites existing item with same attrs
      │     content_type = "text/plain"
      │   )
```

### get_secret

```
dz get DATABASE_URL
      │
      1. SecretService::connect(EncryptionType::Dh)
      │     └─ collection.ensure_unlocked()
      │
      2. collection.search_items({ application, project, profile, key })
      │     → empty result: DotenvzError::KeyNotFound
      │     → first matching item:
      │
      3. item.ensure_unlocked()
      │
      4. item.get_secret() → bytes → UTF-8 decode → return value string
```

### list_secrets

```
dz list
      │
      1. SecretService::connect(EncryptionType::Dh)
      │     └─ collection.ensure_unlocked()
      │
      2. collection.search_items({ application:"dotenvz", project, profile })
      │     → returns all items for this project/profile
      │
      3. for each item:
      │     item_attrs = item.get_attributes()
      │     key = item_attrs["key"]
      │     value = item.get_secret() → UTF-8 decode
      │
      4. return HashMap<key, value>
```

### delete_secret

```
dz rm DATABASE_URL
      │
      1. SecretService::connect(EncryptionType::Dh)
      │     └─ collection.ensure_unlocked()
      │
      2. collection.search_items({ application, project, profile, key })
      │     → empty result: DotenvzError::KeyNotFound
      │     → first matching item:
      │
      3. item.ensure_unlocked()
      │
      4. item.delete()
```

---

## Authentication

No authentication configuration is required. Access is controlled by the keyring
daemon for the active desktop session. dotenvz does not manage daemon passwords,
session tokens, or D-Bus addresses.

**Session requirement:** A Secret Service daemon must be running and reachable on
the session D-Bus. In a standard desktop session (GNOME, KDE, XFCE with GNOME
Keyring installed) this is always satisfied. In **headless environments** (Docker
containers, SSH sessions without a forwarded D-Bus socket, bare CI runners) the
daemon is typically absent and all provider operations will fail with a D-Bus
connection error.

**Headless workarounds:**
- Start the daemon manually: `eval $(gnome-keyring-daemon --start --components=secrets)`
  and export `$DBUS_SESSION_BUS_ADDRESS`.
- Use `secret-tool` or `gnome-keyring-daemon --unlock` to pre-unlock the default
  collection before running dotenvz.
- On CI systems, consider using a cloud provider instead of the OS provider, or
  inject secrets via environment variables directly.

**Keyring lock behaviour:** If the default collection is locked (e.g. after a screen
lock), the daemon will attempt to display an unlock dialog via the desktop. In headless
contexts where no dialog can be shown, `ensure_unlocked()` returns an error which
dotenvz surfaces as `DotenvzError::Provider`.

---

## Error Handling

| Condition                             | dotenvz error                       | Notes                                                            |
|---------------------------------------|-------------------------------------|------------------------------------------------------------------|
| Key does not exist                    | `DotenvzError::KeyNotFound`         | `search_items` returned an empty result                          |
| D-Bus connection failure              | `DotenvzError::Provider("…")`      | Daemon not running or `DBUS_SESSION_BUS_ADDRESS` not set         |
| Collection locked / unlock failed     | `DotenvzError::Provider("…")`      | `ensure_unlocked()` failed; headless environment likely          |
| Secret value is not valid UTF-8       | `DotenvzError::Provider("…")`      | Binary value stored by another application                       |
| Default collection not found          | `DotenvzError::Provider("…")`      | Daemon is running but no default collection exists yet           |
| Provider called on non-Linux platform | `DotenvzError::UnsupportedPlatform` | Stub returns this on macOS / Windows                             |

All `DotenvzError::Provider` payloads include the original `secret-service` error
message. Secret values are never included in error messages.

---

## Key Name Constraints

| Constraint       | Detail                                                                       |
|------------------|------------------------------------------------------------------------------|
| Encoding         | Key names and values must be valid UTF-8.                                    |
| Length           | No hard limit imposed by dotenvz; practical limits are daemon-dependent.     |
| Characters       | No restrictions; D-Bus attribute values accept arbitrary UTF-8 strings.      |
| Case sensitivity | Keys are case-sensitive (`foo` and `FOO` are separate items).                |
| Attribute names  | `application`, `project`, `profile`, and `key` are reserved by dotenvz.     |

---

## Daemon Compatibility

| Daemon           | Status     | Notes                                                                       |
|------------------|------------|-----------------------------------------------------------------------------|
| GNOME Keyring    | Supported  | Default on GNOME desktops; ships with Ubuntu, Fedora, etc.                  |
| KWallet 6        | Supported  | Default on KDE Plasma 6; KWallet implements the Secret Service D-Bus API.   |
| KWallet 5        | Supported  | KDE Plasma 5; same API compatibility.                                       |
| pass / gopass    | Not supported | `pass` does not implement the Secret Service D-Bus protocol.             |
| Bitwarden (CLI)  | Not supported | Does not expose a Secret Service D-Bus server.                           |
| 1Password shell  | Not supported | Does not implement Secret Service D-Bus.                                 |

---

## Rust Crate

```toml
[target.'cfg(target_os = "linux")'.dependencies]
secret-service = { version = "5", features = ["rt-tokio-crypto-rust"] }
```

The `secret-service` crate provides blocking Rust bindings for the Secret Service
1.0 D-Bus specification. The `rt-tokio-crypto-rust` feature enables the Tokio async
runtime and a pure-Rust TLS/crypto backend (no OpenSSL dependency).

**Build note:** This crate brings in a Tokio runtime and pulls several cryptographic
crates (for Diffie-Hellman session encryption with the daemon). Linux builds have
higher compile times than the macOS or Windows providers as a result.

---

## Visibility in the Keyring UI

Secrets stored by dotenvz are visible in native keyring managers:

- **GNOME Keyring / Seahorse:** items appear in the **Passwords** section with
  the label `dotenvz/<project>/<profile>/<key>` and the source application
  attribute `dotenvz`.
- **KDE Wallet Manager:** items appear in the default wallet under the `dotenvz`
  application folder.

Items deleted through the UI will not appear in the next `list_secrets` call,
and no registry reconciliation is required.
