# Architecture — dotenvz

## Overview

`dotenvz` is a single Rust binary that:

1. Reads a per-project `.dotenvz.toml` config from the nearest ancestor directory.
2. Resolves the active profile (CLI flag → config default).
3. Constructs a `SecretProvider` implementation — either a built-in OS provider, a
   custom exec provider, or a cloud provider — based on the active config.
4. Either manages individual secrets (`set`, `get`, `list`, `delete`) or injects the
   full env map into a child process (`exec`, named aliases).

All secret storage backends implement the same `SecretProvider` trait. The rest of
the codebase — command handlers, the env resolver, the process runner — is
completely provider-agnostic.

---

## Module responsibilities

| Module | Responsibility |
|--------|----------------|
| `main.rs` | Entry point. Parses CLI, calls `build_provider()`, dispatches to command handlers. |
| `cli.rs` | `clap`-derived CLI struct. Defines all subcommands and flags. `external_subcommand` captures unrecognised names for alias resolution. |
| `errors.rs` | Single `DotenvzError` enum (via `thiserror`). All modules return `errors::Result<T>`. |
| `config/model.rs` | `DotenvzConfig` struct — serde model for `.dotenvz.toml`. Includes `ProviderConfig` enum and `KNOWN_PROVIDERS` list. |
| `config/loader.rs` | `find_config_file` (walk-up search), `load_config`, `write_config`. |
| `core/project_context.rs` | `ProjectContext` — resolves cwd → config path → config → active profile. Passed into every command handler. |
| `core/command_resolver.rs` | Distinguishes built-in commands from `[commands]` aliases. |
| `core/env_resolver.rs` | Calls `provider.list_secrets()` and returns the env map for injection. |
| `core/process_runner.rs` | Spawns child processes with inherited env + injected secrets overlaid. |
| `providers/secret_provider.rs` | `SecretProvider` trait: `set_secret`, `get_secret`, `list_secrets`, `delete_secret`. |
| `providers/macos_passwords.rs` | macOS default provider — iCloud Keychain / Passwords.app (`kSecAttrSynchronizable`) with silent local-Keychain fallback. Real impl on macOS; stub elsewhere. |
| `providers/macos_keychain.rs` | macOS local-only provider — login Keychain via `security-framework`. Real impl on macOS; stub elsewhere. |
| `providers/linux_secret_service.rs` | Linux provider — Secret Service D-Bus via `secret-service` (blocking). Real impl on Linux; stub elsewhere. |
| `providers/windows_credential.rs` | Windows provider — Credential Manager via `windows-sys` Win32 FFI. Real impl on Windows; stub elsewhere. |
| `providers/exec.rs` | Exec provider — spawns a subprocess and communicates over a JSON stdin/stdout protocol. Cross-platform. |
| `providers/aws_secrets_manager.rs` | AWS Secrets Manager provider — read-only. Cross-platform. |
| `providers/gcp_secret_manager.rs` | Google Cloud Secret Manager provider — read-only. Cross-platform. |
| `providers/azure_key_vault.rs` | Azure Key Vault provider — read-only. Cross-platform. |
| `providers/mock.rs` | In-memory `HashMap` backend used in unit and integration tests. |
| `commands/*` | One file per CLI command. Each takes a `&ProjectContext` and `&dyn SecretProvider`. |

---

## Execution flow

```
dotenvz dev
    │
    ├─ clap: Commands::Alias(["dev"])
    │
    ├─ ProjectContext::resolve()
    │     └─ walk up from cwd → find .dotenvz.toml → parse → pick profile
    │
    ├─ build_provider(&ctx)
    │     ├─ resolve active provider name:
    │     │     a. [profiles.<active>].provider  (if set)
    │     │     b. top-level config.provider
    │     │
    │     ├─ built-in name? (KNOWN_PROVIDERS)
    │     │     "macos-passwords"     → MacOsPasswordsProvider::new()  ← new default
    │     │     "macos-keychain"      → MacOsKeychainProvider::new()
    │     │     "linux-secret-service"→ LinuxSecretServiceProvider::new()
    │     │     "windows-credential"  → WindowsCredentialProvider::new()
    │     │
    │     └─ custom name? (ctx.config.providers map)
    │           type = "exec"                → ExecProvider::new(cfg)
    │           type = "aws-secrets-manager" → AwsSecretsManagerProvider::new(cfg)
    │           type = "gcp-secret-manager"  → GcpSecretManagerProvider::new(cfg)
    │           type = "azure-key-vault"     → AzureKeyVaultProvider::new(cfg)
    │
    ├─ resolve_command("dev", &config)
    │     └─ ResolvedCommand::Alias { resolved: "next dev" }
    │
    ├─ env_resolver::resolve_env(&provider, project, profile)
    │     └─ provider.list_secrets() → HashMap<String, String>
    │
    └─ process_runner::run_command_string("next dev", &env)
          └─ Command::new("next").arg("dev").envs(&env).status()
```

`dotenvz exec -- cargo run` follows the same path via `Commands::Exec`.

---

## Config flow

```
.dotenvz.toml
    │
    ├─ config/loader.rs: find_config_file() walk-up search
    ├─ config/loader.rs: load_config() → toml::from_str()
    └─ config/model.rs: DotenvzConfig

DotenvzConfig
    ├─ project         → secret namespace prefix (all providers)
    ├─ provider        → default provider name (auto-set by `dotenvz init`)
    ├─ default_profile → fallback when --profile not given
    ├─ import_file     → path used by `dotenvz import`
    ├─ schema_file     → (future) validation key list
    ├─ commands        → alias name → shell command string
    └─ providers       → map of provider alias → ProviderConfig

ProviderConfig (enum, keyed by the `type` field)
    ├─ Exec   { command, args, timeout_ms, env, working_dir }
    ├─ Aws    { region, prefix }
    ├─ Gcp    { project_id, prefix }
    └─ Azure  { vault_url, prefix }
```

The `type` field in each `[providers.<name>]` block drives serde deserialisation
to the correct `ProviderConfig` variant. The `type` discriminant is also used by
`build_provider()` to select the implementation struct.

---

## Provider abstraction

```rust
pub trait SecretProvider: Send + Sync {
    fn set_secret(&self, project: &str, profile: &str, key: &str, value: &str) -> Result<()>;
    fn get_secret(&self, project: &str, profile: &str, key: &str)              -> Result<String>;
    fn list_secrets(&self, project: &str, profile: &str)                       -> Result<HashMap<String, String>>;
    fn delete_secret(&self, project: &str, profile: &str, key: &str)           -> Result<()>;
}
```

The provider is constructed once in `main.rs` and passed as `Box<dyn SecretProvider>`
(then borrowed as `&dyn SecretProvider`) into every command handler. Swapping
backends requires only changing `build_provider()`.

`dotenvz init` calls `default_provider()`, which uses `cfg!()` macros to write the
correct provider string for the host OS at compile time.

### OS provider storage layouts

**macOS — iCloud Keychain / Passwords.app (`macos-passwords`, default)**
```
kSecAttrService        = "dotenvz.<project>.<profile>"
kSecAttrAccount        = "<key>"
kSecValueData          = "<value>" (UTF-8)
kSecAttrSynchronizable = true  ← synced via iCloud; appears in Passwords.app

Fallback: if iCloud is unavailable, the same layout is written to the local
login Keychain without the synchronizable flag.

Sentinel registry account (for list_secrets — same as macos-keychain):
  kSecAttrAccount = "__dotenvz_idx__"  → newline-separated key names
  (stored with kSecAttrSynchronizable = true; local copy maintained on fallback)
```

**macOS — local login Keychain (`macos-keychain`)**
```
kSecAttrService = "dotenvz.<project>.<profile>"
kSecAttrAccount = "<key>"
kSecValueData   = "<value>" (UTF-8)

Sentinel registry account (for list_secrets):
  kSecAttrAccount = "__dotenvz_idx__"  → newline-separated key names
```

**Linux — Secret Service**
```
Item attributes:
  application = "dotenvz"
  project     = "<project>"
  profile     = "<profile>"
  key         = "<key>"
Item secret   = "<value>" (UTF-8, content-type "text/plain")
Item label    = "dotenvz/<project>/<profile>/<key>"

(Native attribute search used for list_secrets — no key registry needed.)
```

**Windows — Credential Manager**
```
Type       = CRED_TYPE_GENERIC
TargetName = "dotenvz/<project>/<profile>/<key>"
Blob       = "<value>" (UTF-8)
Persist    = CRED_PERSIST_LOCAL_MACHINE

(CredEnumerateW with prefix wildcard used for list_secrets — no key registry needed.)
```

---

## Provider categories

| Dimension            | OS providers                    | Cloud providers                      | Exec providers              |
|----------------------|---------------------------------|--------------------------------------|-----------------------------|
| Storage              | OS keystore (in-process FFI)    | Remote HTTP/gRPC call                | External process (IPC)      |
| Blocking I/O         | Synchronous syscall             | Network call (blocking SDK wrapper)  | Subprocess blocking I/O     |
| Authentication       | OS user session (automatic)     | Ambient credentials (IAM, ADC, MSI) | Provider manages internally |
| Write support        | Full (`set`, `get`, `list`, `delete`) | Read-only (`set`/`delete` → `UnsupportedOperation`) | Depends on impl |
| Platform gate        | Yes (`#[cfg(target_os)]`)       | No (cross-platform)                  | No (cross-platform)         |
| Failure modes        | Local I/O / Keychain errors     | Network, auth, rate-limit            | Process errors, timeouts    |

### Async vs. blocking

dotenvz uses a synchronous single-threaded model. Cloud SDKs (aws-sdk-rust,
google-cloud-rust, azure-sdk-for-rust) are async (`tokio`-based).

Each cloud provider creates a minimal single-threaded `tokio` runtime scoped to the
duration of the dotenvz invocation and calls `runtime.block_on(async_fn)`. The
`SecretProvider` trait signature remains synchronous. This is zero-cost when no
cloud provider is active and avoids a full async migration of the codebase.

Full async migration (`#[tokio::main]`, async trait methods) is deferred to a future
milestone.

### Error mapping

| Source error                              | `DotenvzError` variant                  |
|-------------------------------------------|-----------------------------------------|
| Key absent in OS store / 404 from cloud   | `KeyNotFound { key, profile }`          |
| OS permission / 403 from cloud            | `Provider("access denied: …")`         |
| Cloud network / timeout                   | `Provider("network error: …")`         |
| Cloud auth failure                        | `Provider("auth error: …")`            |
| Binary secret not valid UTF-8             | `Provider("…")`                        |
| Exec provider: non-zero exit, no JSON     | `Provider("exec provider failed: …")`  |
| Exec provider: malformed JSON             | `Provider("exec provider returned malformed JSON: …")` |
| Exec provider: timeout                    | `Provider("exec provider timed out after Nms")` |
| Process spawn failure                     | `ProcessExec("…")`                     |
| `set_secret`/`delete_secret` on cloud    | `UnsupportedOperation`                  |
| Provider called on wrong OS               | `UnsupportedPlatform`                   |

Secret values are never included in error messages.

---

## Exec provider flow

```
ExecProvider::list_secrets(project, profile)
      │
      ├─ spawn(<command> [args…])
      ├─ write stdin: {"action":"list","project":"…","profile":"…"}\n
      ├─ close stdin
      ├─ read stdout (subject to timeout_ms)
      ├─ parse JSON → {"ok":true,"keys":["A","B","C"]}
      ├─ wait for process exit (code 0)
      │
      └─ for each key in ["A","B","C"]:
            ExecProvider::get_secret(project, profile, key)
                  ├─ spawn(...)
                  ├─ write {"action":"get","project":"…","profile":"…","key":"A"}\n
                  ├─ read stdout → {"ok":true,"value":"val-A"}
                  └─ wait for exit
      │
      └─ return HashMap {"A":"val-A","B":"val-B","C":"val-C"}
```

`list_secrets` performs N+1 subprocess launches (one for list, one per key).
See `docs/CUSTOM_PROVIDER_PROTOCOL.md` for the full JSON wire protocol.

There are **two distinct uses** of `std::process::Command` in dotenvz:

| Use site | Purpose |
|----------|---------|
| `core/process_runner.rs` | Runs the user's application with injected env vars |
| `providers/exec.rs` | Communicates with the secret provider subprocess |

These are entirely independent and share no state.

---

## Adding a new command

1. Add a variant to `Commands` in `src/cli.rs`.
2. Create `src/commands/<name>.rs` with a `pub fn run(fn(&ProjectContext, &dyn SecretProvider))`.
3. Add `pub mod <name>;` to `src/commands/mod.rs`.
4. Add the match arm in `src/main.rs`.

---

## Adding a new built-in OS provider

1. Create `src/providers/<name>.rs` implementing `SecretProvider`.
   - Gate the real implementation with `#[cfg(target_os = "…")]`.
   - Add a `#[cfg(not(target_os = "…"))]` stub that returns `DotenvzError::UnsupportedPlatform`.
2. Add `pub mod <name>;` to `src/providers/mod.rs`.
3. Add a `cfg(target_os)` branch to `build_provider()` in `main.rs`.
4. Add the provider key string to `KNOWN_PROVIDERS` and `default_provider()` in `config/model.rs`.
5. Add platform-gated dependencies to `Cargo.toml` under `[target.'cfg(…)'.dependencies]`.

## Adding a new cloud or custom provider

1. Create `src/providers/<name>.rs` implementing `SecretProvider`.
   - No platform gate; the provider is cross-platform.
   - `set_secret` and `delete_secret` should return `DotenvzError::UnsupportedOperation`
     if write is not supported.
2. Add `pub mod <name>;` to `src/providers/mod.rs`.
3. Add a new variant to `ProviderConfig` in `config/model.rs` and update `validate()`.
4. Add a match arm to `build_provider()` in `main.rs`.
5. Add optional feature-gated dependencies to `Cargo.toml`.

---

## Future extensions

| Extension | Notes |
|-----------|-------|
| Profile inheritance | A `staging` profile inherits `dev` defaults and overlays its own values |
| Schema validation | Warn when a key in `schema_file` is missing from the provider |
| Shell hooks | `eval "$(dotenvz hook zsh)"` to auto-inject on `cd` |
| VS Code extension | Read context from dotenvz for launch configurations |
| Full async `SecretProvider` trait | Replace sync trait methods with `async fn`; migrate `main.rs` to `#[tokio::main]` |
| Parallel secret fetching | Cloud and exec `list_secrets` fetches secrets concurrently with `tokio::join_all` |
| Secret caching | Cache results in memory within a process run to avoid redundant API calls |
| Cloud write support | Opt-in `set_secret` / `delete_secret` for cloud providers |
