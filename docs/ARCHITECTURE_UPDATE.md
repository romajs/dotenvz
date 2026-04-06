# Architecture Update — Exec and Cloud Provider Integration

> **Superseded.**
> The content of this document has been merged into [`docs/ARCHITECTURE.md`](ARCHITECTURE.md).
> This file is retained for historical reference only.

---

## Existing Architecture (Summary)

```
main.rs
  └─ build_provider()          → Box<dyn SecretProvider>
        └─ MacOsKeychainProvider   (macOS)
        └─ LinuxSecretServiceProvider (Linux)
        └─ WindowsCredentialProvider  (Windows)

env_resolver::resolve_env()
  └─ provider.list_secrets(project, profile)
        └─ HashMap<String, String>

process_runner::run_command_string(cmd, &env)
```

Provider selection is currently platform-gated (`#[cfg(target_os = "...")]`).
Exec and cloud providers are **not** platform-gated; they work on any host OS.

---

## ExecProvider Integration

### What ExecProvider does

`ExecProvider` implements `SecretProvider` by spawning a local subprocess and
communicating with it over a JSON stdin/stdout protocol. It is the backing
implementation for `[providers.<name>]` entries with `type = "exec"` in
`.dotenvz.toml`. See `docs/CUSTOM_PROVIDER_PROTOCOL.md` for the full wire protocol.

### Where ExecProvider fits

```
src/providers/exec.rs
  └─ ExecProvider
        ├─ config: ExecProviderConfig   (command, args, timeout_ms, env, working_dir)
        └─ impl SecretProvider
              ├─ get_secret    → spawn, send {"action":"get",...}, parse response
              ├─ set_secret    → spawn, send {"action":"set",...}, parse response
              ├─ list_secrets  → spawn, send {"action":"list",...}, parse keys
              └─ delete_secret → spawn, send {"action":"rm",...}, parse response
```

### Interaction with `SecretProvider` trait

`ExecProvider` implements the same four-method trait as every other provider:

```
pub trait SecretProvider: Send + Sync {
    fn set_secret(&self, project: &str, profile: &str, key: &str, value: &str) -> Result<()>;
    fn get_secret(&self, project: &str, profile: &str, key: &str) -> Result<String>;
    fn list_secrets(&self, project: &str, profile: &str) -> Result<HashMap<String, String>>;
    fn delete_secret(&self, project: &str, profile: &str, key: &str) -> Result<()>;
}
```

No trait changes are required. All existing command handlers and the `env_resolver`
call the trait methods without knowing the provider is exec-backed.

### How `process_runner` interacts with `ExecProvider`

There are **two distinct uses** of `std::process::Command` in dotenvz:

| Use site              | Purpose                                             |
|-----------------------|-----------------------------------------------------|
| `core/process_runner.rs` | Runs the user's application with injected env vars |
| `providers/exec.rs`      | Communicates with the secret provider subprocess   |

These are entirely independent. `ExecProvider` does not call or share state with
`process_runner`. Each exec provider operation spawns its own short-lived child
process, writes the JSON request to its stdin, reads the JSON response from its
stdout, and waits for the process to exit — all within a single `SecretProvider`
method call.

### How `env_resolver` uses `ExecProvider`

`env_resolver::resolve_env` calls `provider.list_secrets(project, profile)`.
When the active provider is an `ExecProvider`:

```
resolve_env(provider, project, profile)
  └─ ExecProvider::list_secrets(project, profile)
        └─ spawn subprocess
        └─ write {"action":"list","project":"...","profile":"..."}
        └─ read stdout → parse {"ok":true,"keys":["A","B","C"]}
        └─ for each key k:
              ExecProvider::get_secret(project, profile, k)
                └─ spawn subprocess
                └─ write {"action":"get","project":"...","profile":"...","key":"k"}
                └─ read stdout → parse {"ok":true,"value":"..."}
        └─ return HashMap<String, String>
```

This means `list_secrets` performs N+1 subprocess launches (one for list, one per
key). This is acceptable for exec providers because they are intended for scenarios
where the subprocess launch overhead is negligible relative to the operation being
tunnelled. If N+1 is a concern, the provider implementation can handle `list` by
returning key/value pairs directly in a future protocol extension.

### Error propagation

`ExecProvider` maps protocol-level outcomes to `DotenvzError` as follows:

| Condition                              | `DotenvzError` variant                              |
|----------------------------------------|-----------------------------------------------------|
| `{"ok":false,"error":{"code":"NOT_FOUND",...}}` | `KeyNotFound`                        |
| `{"ok":false,"error":{"code":"ACCESS_DENIED",...}}` | `Provider("access denied: …")`   |
| `{"ok":false,"error":{"code":"INVALID_REQUEST",...}}` | `Provider("invalid request: …")` |
| `{"ok":false,"error":{"code":"INTERNAL_ERROR",...}}` | `Provider("provider error: …")`  |
| Non-zero exit + no valid JSON          | `Provider("exec provider failed: exit code N")`     |
| Malformed JSON on stdout               | `Provider("exec provider returned malformed JSON: …")` |
| Timeout                                | `Provider("exec provider timed out after Nms")`     |
| Process spawn failure                  | `ProcessExec` (existing variant)                    |

`DotenvzError::ProcessExec` is used only when the process cannot be spawned at all
(binary not found, permission denied on the executable). Protocol-level errors use
`DotenvzError::Provider`.

---

---

## Integration Points

### 1. `SecretProvider` trait — unchanged

Cloud providers implement the same trait as all existing providers:

```rust
pub trait SecretProvider: Send + Sync {
    fn set_secret(&self, project: &str, profile: &str, key: &str, value: &str) -> Result<()>;
    fn get_secret(&self, project: &str, profile: &str, key: &str) -> Result<String>;
    fn list_secrets(&self, project: &str, profile: &str) -> Result<HashMap<String, String>>;
    fn delete_secret(&self, project: &str, profile: &str, key: &str) -> Result<()>;
}
```

`set_secret` and `delete_secret` return `DotenvzError::UnsupportedOperation` for
cloud providers in the MVP. All other code paths that receive a `&dyn SecretProvider`
work without modification.

### 2. New provider structs

Three new structs are added under `src/providers/`:

| File                          | Struct                        | Provider type string         |
|-------------------------------|-------------------------------|------------------------------|
| `aws_secrets_manager.rs`      | `AwsSecretsManagerProvider`   | `"aws-secrets-manager"`      |
| `gcp_secret_manager.rs`       | `GcpSecretManagerProvider`    | `"gcp-secret-manager"`       |
| `azure_key_vault.rs`          | `AzureKeyVaultProvider`       | `"azure-key-vault"`          |

Each struct holds its deserialised `ProviderConfig` and a lazily initialised (or
eagerly constructed) SDK client. The client is created in `new()` and reused for
all calls within a single dotenvz invocation.

### 3. `ProviderConfig` model extension

The existing `ProviderConfig` struct in `src/config/model.rs` is specific to `exec`
providers. Cloud providers require different fields.

Two approaches are possible:

**Option A — Enum-based config (recommended)**

Replace `ProviderConfig` with an enum:

```
ProviderConfig::Exec   { command, args, timeout_ms, env, working_dir }
ProviderConfig::Aws    { region, prefix }
ProviderConfig::Gcp    { project_id, prefix }
ProviderConfig::Azure  { vault_url, prefix }
```

This gives compile-time exhaustiveness checking when new providers are added.

**Option B — Flat struct with optional fields**

Keep a single struct and add optional fields for each provider type.
Simpler to deserialise from TOML but loses type safety.

Option A is preferred. The `type` discriminant field drives the serde deserialisation.

### 4. `build_provider()` — provider factory

`build_provider()` in `main.rs` is extended to handle cloud provider aliases.
The resolution order after loading `ProjectContext`:

```
1. Determine active provider name:
     a. [profiles.<active>].provider  (if set)
     b. top-level config.provider

2. Match against KNOWN_PROVIDERS (built-in):
     "macos-keychain"         → MacOsKeychainProvider::new()
     "linux-secret-service"   → LinuxSecretServiceProvider::new()
     "windows-credential"     → WindowsCredentialProvider::new()

3. Match against config.providers (custom / cloud):
     look up name in ctx.config.providers
     match ProviderConfig::type:
       "exec"                  → ExecProvider::new(cfg)
       "aws-secrets-manager"   → AwsSecretsManagerProvider::new(cfg)
       "gcp-secret-manager"    → GcpSecretManagerProvider::new(cfg)
       "azure-key-vault"       → AzureKeyVaultProvider::new(cfg)
       _                       → DotenvzError::ConfigParse(...)

4. Return Box<dyn SecretProvider>
```

The rest of `main.rs` is unchanged. `env_resolver` and all command handlers receive
the `Box<dyn SecretProvider>` and call the trait methods without knowing whether
the backend is local or cloud.

### 5. `env_resolver` — unchanged

`resolve_env` calls `provider.list_secrets()`. Cloud providers return the same
`HashMap<String, String>` type. No changes are required in `env_resolver.rs`.

---

## Provider Categories

### Comparison table

| Dimension               | Local providers               | Cloud providers                      | Exec providers           |
|-------------------------|-------------------------------|--------------------------------------|--------------------------|
| Storage                 | OS keystore (in-process FFI)  | Remote HTTP/gRPC call                | External process (IPC)   |
| Blocking I/O model      | Synchronous blocking          | Network call (blocking SDK)          | Subprocess blocking I/O  |
| Authentication          | OS user session               | Ambient credentials (IAM, ADC, MSI) | N/A (provider manages)   |
| Read-write              | Full                          | Read-only (MVP)                      | Depends on implementation |
| Platform gate           | Yes (`#[cfg(target_os)]`)     | No (cross-platform)                  | No (cross-platform)       |
| Failure modes           | Local I/O errors              | Network, auth, rate-limit            | Process errors, timeouts  |

### Async vs. blocking

dotenvz currently uses a synchronous single-threaded model. Cloud SDKs for Rust
(aws-sdk-rust, google-cloud-rust, azure-sdk-for-rust) are async (`tokio`-based).

Two strategies for bridging this:

**Strategy A (recommended for MVP): `tokio::runtime::Runtime::block_on`**

Each cloud provider implementation creates a minimal single-threaded `tokio` runtime
for the duration of the dotenvz invocation and calls `runtime.block_on(async_fn)`.
This is zero-cost when there is no cloud provider and avoids threading the async
runtime throughout the entire codebase.

**Strategy B: Full async migration**

Convert `main.rs` to `#[tokio::main]` and make `SecretProvider` async.
This is the right long-term direction but is a broader change deferred to a
future milestone.

The `SecretProvider` trait signature remains synchronous in the MVP.
Cloud provider implementations are responsible for blocking internally on their
async SDK calls.

### Error propagation

Cloud providers map SDK-level errors to `DotenvzError` variants:

| SDK error type                            | `DotenvzError` variant      |
|-------------------------------------------|-----------------------------|
| Resource not found (404 / NOT_FOUND)      | `KeyNotFound`               |
| Permission denied (403 / PERMISSION_DENIED) | `Provider("access denied: …")` |
| Network / timeout                         | `Provider("network error: …")` |
| Authentication failure                    | `Provider("auth error: …")` |
| All other SDK errors                      | `Provider(<message>)`       |

Error messages include the SDK error string but never include secret values.

### Retries

Cloud SDKs perform automatic retries with exponential back-off for transient errors
(network blip, rate-limit throttle). dotenvz delegates retry logic entirely to the
SDK. No custom retry loop is implemented in dotenvz itself for the MVP.

If the SDK exhausts its retry budget, it surfaces an error that dotenvz maps to
`DotenvzError::Provider` and reports to the user.

---

## Updated Module Map

| Module                              | Change                                              |
|-------------------------------------|-----------------------------------------------------|
| `src/providers/secret_provider.rs`  | Add `UnsupportedOperation` variant to `DotenvzError`; no trait change |
| `src/providers/exec.rs`             | **New** — `ExecProvider` (subprocess JSON protocol) |
| `src/providers/aws_secrets_manager.rs` | **New** — `AwsSecretsManagerProvider`            |
| `src/providers/gcp_secret_manager.rs`  | **New** — `GcpSecretManagerProvider`             |
| `src/providers/azure_key_vault.rs`     | **New** — `AzureKeyVaultProvider`                |
| `src/providers/mod.rs`              | Export `ExecProvider` and the three new cloud provider structs |
| `src/config/model.rs`               | Extend `ProviderConfig` to support cloud types; update `KNOWN_PROVIDERS`; update `validate()` |
| `src/errors.rs`                     | Add `UnsupportedOperation` variant                  |
| `src/main.rs`                       | Extend `build_provider()` to handle cloud provider types |
| `Cargo.toml`                        | Add `aws-sdk-secretsmanager`, `google-cloud-secretmanager`, `azure_security_keyvault_secrets`, `tokio` as optional features |

`src/core/`, `src/commands/`, `src/cli.rs` — **no changes required**.

---

## Updated Execution Flow

```
dotenvz exec -- my-service
      │
      ├─ ProjectContext::resolve()
      │
      ├─ build_provider(&ctx)
      │     ├─ resolve active provider name (profile override > top-level)
      │     ├─ built-in?  → local OS provider (MacOS/Linux/Windows)
      │     └─ in providers map?
      │           ├─ type = "exec"                → ExecProvider::new(cfg)
      │           ├─ type = "aws-secrets-manager" → AwsSecretsManagerProvider::new(cfg)
      │           ├─ type = "gcp-secret-manager"  → GcpSecretManagerProvider::new(cfg)
      │           └─ type = "azure-key-vault"     → AzureKeyVaultProvider::new(cfg)
      │
      ├─ env_resolver::resolve_env(&provider, project, profile)
      │     └─ provider.list_secrets()
      │           ├─ [exec]   spawn subprocess, JSON protocol
      │           └─ [cloud]  SDK call → enumerate keys, fetch values
      │
      └─ process_runner::run_command_string("my-service", &env)
```

### ExecProvider flow (expanded)

```
ExecProvider::list_secrets(project, profile)
      │
      ├─ spawn("/usr/local/bin/my-provider", ["--mode", "dotenvz"])
      ├─ write stdin: {"action":"list","project":"my-app","profile":"dev"}\n
      ├─ close stdin
      ├─ read stdout until EOF (subject to timeout_ms)
      ├─ parse JSON → {"ok":true,"keys":["A","B","C"]}
      ├─ wait for process exit
      │
      └─ for each key in ["A","B","C"]:
            ExecProvider::get_secret(project, profile, key)
                  ├─ spawn(...)
                  ├─ write {"action":"get","project":"my-app","profile":"dev","key":"A"}\n
                  ├─ close stdin
                  ├─ read stdout → parse {"ok":true,"value":"val-A"}
                  └─ wait for exit
      │
      └─ return HashMap {"A":"val-A","B":"val-B","C":"val-C"}
```

---

---

## Future Considerations

- **Full async `SecretProvider` trait** — Replace `fn get_secret(…) -> Result<String>`
  with `async fn get_secret(…) -> Result<String>`. Requires migrating `main.rs` to
  `#[tokio::main]` and updating all call sites.
- **Parallel secret fetching** — `list_secrets` for cloud providers currently fetches
  secrets sequentially. Future work: fetch concurrently with `tokio::join_all`.
- **Secret caching** — Cache results in memory within a process run to avoid redundant
  API calls when multiple commands share the same provider invocation.
- **Write support** — `set_secret` and `delete_secret` as opt-in cloud operations.
