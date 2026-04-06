# OS Target Providers — Overview

## Overview

dotenvz uses the operating system's native secret store as its default backend.
Secrets are written to, read from, and deleted from the OS keystore transparently —
no extra services, daemons, or cloud accounts required.

OS providers are used when:

- Developing locally on a developer machine.
- Running scripts on a single-user workstation or build agent.
- Preferring zero-network-dependency secret management.

### Supported OS providers

| Provider key        | Backend                                    | Platform        |
|---------------------|--------------------------------------------|-----------------|
| `macos-keychain`    | macOS Keychain (Security.framework)        | macOS           |
| `linux-secret-service` | Secret Service D-Bus API (GNOME Keyring / KWallet) | Linux      |
| `windows-credential` | Windows Credential Manager (Win32 Cred*)  | Windows         |

---

## OS Providers vs Cloud Providers

| Dimension              | OS providers                                     | Cloud providers                                         |
|------------------------|--------------------------------------------------|---------------------------------------------------------|
| Storage location       | OS-native keystore (Keychain, Secret Service, …) | Cloud-hosted secret store                               |
| Authentication         | OS user session (automatic)                      | IAM role, workload identity, or managed identity        |
| Write support          | Full (`set`, `get`, `list`, `delete`)            | Read-only in MVP                                        |
| Latency                | Microseconds (local syscall)                     | Milliseconds (network round-trip)                       |
| Availability           | Always available (offline)                       | Requires network + cloud service availability           |
| Ideal environment      | Local developer machine                          | CI/CD pipelines, containers, serverless functions       |
| Credential management  | OS session — dotenvz needs no credentials        | Ambient IAM credentials required                        |
| Secret isolation       | Per-project, per-profile namespace               | Per-project, per-profile namespace                      |

---

## Secret Namespace Model

All three OS providers share the same logical namespace:

```
project   →  the `project` field in `.dotenvz.toml`
profile   →  the active profile (default: `default`)
key       →  the environment variable name (e.g. `DATABASE_URL`)
```

This means the same key name (`DATABASE_URL`) can coexist in multiple profiles
(`dev`, `staging`, `production`) without collision.

---

## macOS Keychain

### Backend

The macOS provider uses the **Security.framework Generic Password** API via the
`security-framework` crate. Secrets are stored in the login Keychain (the same store
used by Safari, Xcode, and system services).

### Storage layout

Each secret is stored as a **Generic Password** item inside the login Keychain:

| Keychain attribute  | Value                         |
|---------------------|-------------------------------|
| `kSecAttrService`   | `dotenvz.<project>.<profile>` |
| `kSecAttrAccount`   | `<key>`                       |
| `kSecValueData`     | UTF-8 encoded `<value>`       |

### Key registry

macOS Keychain does not expose a native "list all accounts for a service" API without
CoreFoundation type-casting. To support `list_secrets` efficiently, the macOS provider
maintains a **key registry**: a newline-delimited list of key names stored under the
sentinel account `__dotenvz_idx__` in the same service namespace.

The registry is updated on every `set_secret` and `delete_secret` call. If an item is
deleted directly via the Keychain Access UI, the registry may drift until the next
`set` or `delete` operation reconciles it.

### Authentication

No additional authentication is required. Access is controlled by the macOS user
session. On first access, macOS may prompt for the login password to unlock the
Keychain (this is a system-level prompt handled entirely by macOS).

### Rust crate

```toml
[target.'cfg(target_os = "macos")'.dependencies]
security-framework = "2"
core-foundation = "0.9"
```

### Execution flow

```
dz set DATABASE_URL postgres://...
      │
      1. service = "dotenvz.<project>.<profile>"
      │
      2. upsert_password(service, key, value)
      │     └─ set_generic_password()
      │           → on errSecDuplicateItem: delete_generic_password() then retry
      │
      3. registry_add(service, key)
            └─ read __dotenvz_idx__, append key, write back
```

### Known trade-offs

- The key registry can drift if items are modified directly in Keychain Access.
- Keychain unlock prompts are controlled by macOS and cannot be suppressed by dotenvz.
- Items persist across user logout; they are tied to the login Keychain, not the session.

---

## Linux Secret Service

### Backend

The Linux provider uses the **Secret Service D-Bus API** via the `secret-service` crate
(blocking mode, tokio + rustls crypto backend). It stores secrets in the default
collection of whatever Secret Service implementation is active — typically **GNOME
Keyring** or **KWallet** depending on the desktop environment.

### Storage layout

Each secret is stored as an **item** with four D-Bus attributes:

| Attribute     | Value                    |
|---------------|--------------------------|
| `application` | `"dotenvz"` (constant)   |
| `project`     | project name from config |
| `profile`     | active profile name      |
| `key`         | env-var key name         |

The item **label** (human-readable) is: `dotenvz/<project>/<profile>/<key>`

Because Secret Service supports native attribute-based search, **no separate key
registry is needed** — `list_secrets` queries by `(application, project, profile)`.

### Authentication

Access is controlled by the desktop keyring (GNOME Keyring or KWallet). On first use
in a session, the keyring daemon may prompt the user for its unlock password. In
headless environments (CI, SSH sessions), the daemon must be pre-unlocked or the
`SECRET_SERVICE_*` environment variables must be configured to point to a valid session.

### Rust crate

```toml
[target.'cfg(target_os = "linux")'.dependencies]
secret-service = { version = "5", features = ["rt-tokio-crypto-rust"] }
```

### Execution flow

```
dz set DATABASE_URL postgres://...
      │
      1. SecretService::connect(EncryptionType::Dh)
      │
      2. collection = ss.get_default_collection()
      │     └─ collection.ensure_unlocked()
      │
      3. attrs = { application, project, profile, key }
      │
      4. collection.create_item(label, attrs, value, replace=true, "text/plain")
```

### Known trade-offs

- Requires a running Secret Service daemon (GNOME Keyring, KWallet, or compatible).
- Headless environments (Docker, SSH without `-A`) may not have a daemon running.
- The `rt-tokio-crypto-rust` feature adds a Tokio runtime and `rustls`; build times
  on Linux are higher than on the other platforms.
- KWallet behaviour can differ from GNOME Keyring in subtle ways (e.g. collection
  naming, auto-lock policy).

---

## Windows Credential Manager

### Backend

The Windows provider uses the **Win32 Credential API** (`Cred*` family) via the
`windows-sys` crate. Secrets are stored as **Generic** (`CRED_TYPE_GENERIC`) credentials
with `CRED_PERSIST_LOCAL_MACHINE` persistence.

### Storage layout

| Credential attribute | Value                               |
|----------------------|-------------------------------------|
| Type                 | `CRED_TYPE_GENERIC`                 |
| `TargetName`         | `dotenvz/<project>/<profile>/<key>` |
| `CredentialBlob`     | UTF-8 encoded secret value          |
| `Persist`            | `CRED_PERSIST_LOCAL_MACHINE`        |

### Key registry

Windows Credential Manager supports **prefix wildcard filters** natively via
`CredEnumerateW("dotenvz/<project>/<profile>/*", …)`. No separate key registry is
required; `list_secrets` delegates directly to a single Win32 enumerate call.

### Authentication

Credentials are scoped to the Windows user account. No additional authentication is
required at runtime. The Credential Manager UI (`Control Panel → Credential Manager`)
can be used to inspect or delete stored values.

### Rust crate

```toml
[target.'cfg(target_os = "windows")'.dependencies]
windows-sys = { version = "0.61", features = [
    "Win32_Security_Credentials",
    "Win32_Foundation",
] }
```

### Execution flow

```
dz set DATABASE_URL postgres://...
      │
      1. target = "dotenvz/<project>/<profile>/<key>" (UTF-16 null-terminated)
      │
      2. Build CREDENTIALW {
      │     Type = CRED_TYPE_GENERIC,
      │     TargetName = target,
      │     CredentialBlob = value (UTF-8 bytes),
      │     Persist = CRED_PERSIST_LOCAL_MACHINE,
      │   }
      │
      3. CredWriteW(&cred, 0)
            → on failure: return DotenvzError::Provider("CredWriteW failed (error N)")
```

### Known trade-offs

- `CRED_PERSIST_LOCAL_MACHINE` ties credentials to the machine, not just the user
  profile. On shared machines, the effective scope is (machine, user account).
- `CredentialBlob` is limited to **2560 bytes** by the Win32 API. Secret values larger
  than this limit will fail at the OS level with `ERROR_BAD_LENGTH`.
- The `TargetName` namespace is global within the user account; prefixing with
  `dotenvz/` prevents collisions with other applications.
- Credentials are visible in the Credential Manager UI to the logged-in user.

---

## Cross-Platform Behaviour Summary

| Operation        | macOS Keychain                     | Linux Secret Service              | Windows Credential Manager         |
|------------------|------------------------------------|-----------------------------------|-------------------------------------|
| `set_secret`     | `set_generic_password` (upsert)    | `collection.create_item(replace)` | `CredWriteW`                        |
| `get_secret`     | `get_generic_password`             | `collection.search_items` (first) | `CredReadW`                         |
| `list_secrets`   | registry lookup + multi-get        | `collection.search_items` (all)   | `CredEnumerateW` (wildcard prefix)  |
| `delete_secret`  | `delete_generic_password`          | `item.delete()`                   | `CredDeleteW`                       |
| Key enumeration  | Explicit registry (`__dotenvz_idx__`) | Native attribute search        | Native prefix wildcard              |
| Non-native OS    | Returns `UnsupportedPlatform`      | Returns `UnsupportedPlatform`     | Returns `UnsupportedPlatform`       |

---

## Execution Flow (shared path)

The flow below applies identically across all three OS providers once the correct
provider is instantiated:

```
dotenvz exec -- my-service
      │
      1. ProjectContext::resolve()
      │     └─ walk cwd → find .dotenvz.toml → parse → pick active profile
      │
      2. build_provider(&ctx)
      │     └─ read ctx.config.provider (or profile-level override)
      │     └─ match compile target:
      │           cfg(target_os = "macos")   → MacOsKeychainProvider
      │           cfg(target_os = "linux")   → LinuxSecretServiceProvider
      │           cfg(target_os = "windows") → WindowsCredentialProvider
      │
      3. env_resolver::resolve_env(&provider, project, profile)
      │     └─ provider.list_secrets(project, profile)
      │           └─ fetch all keys stored under (project, profile)
      │           └─ return HashMap<String, String>
      │
      4. process_runner::run_command_string("my-service", &env)
            └─ spawn child with merged env (process.env + injected secrets)
```

Steps 1 and 4 are identical to the cloud provider flow. The OS-specific work is
entirely encapsulated in step 3 inside each provider implementation.

---

## Security Summary

- Secret values never leave the local machine (no network calls, no cloud dependency).
- Values are held in memory only long enough to build the env map and spawn the child
  process.
- Secret values are never written to disk by dotenvz, logged, or surfaced in
  `--dry-run` output (dry-run shows key names as `<redacted>`).
- Access control is delegated to the OS: macOS login Keychain policies, Linux keyring
  daemon ACLs, or Windows user account scope.
- dotenvz does not implement its own encryption; it relies entirely on the OS keystore
  encryption layer.

See [Security Considerations] in `IMPLEMENTATION_GUIDE.md` for a detailed treatment.
