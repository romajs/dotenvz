# Implementation Guide — Exec and Cloud Providers for dotenvz

## Overview

This guide walks through each step required to add exec provider and cloud provider
support to dotenvz. It is intentionally structured as a task list, not a code dump.
Each step identifies what to build, where it lives, and what to watch out for.

No implementation code is included. Refer to `docs/CUSTOM_PROVIDER_PROTOCOL.md`
for the exec provider wire protocol spec and to each cloud SDK's official
documentation for cloud-specific API surfaces.

---

## Prerequisites

Before starting, ensure you are familiar with:

- The existing `SecretProvider` trait in `src/providers/secret_provider.rs`
- The `ProviderConfig` model in `src/config/model.rs`
- `build_provider()` in `src/main.rs`
- `DotenvzError` in `src/errors.rs`

---

## Part A — ExecProvider

These steps implement the exec provider. They are independent of the cloud provider
steps in Part B and can be implemented first or in isolation.

---

## Step A1 — Create `ExecProvider`

**Location:** `src/providers/exec.rs`

Create a new file and define a struct `ExecProvider` that holds the deserialized
exec provider configuration.

### What to do

1. Define a struct `ExecProvider` with fields mirroring the config model:
   - `command: String` — absolute path to the executable.
   - `args: Vec<String>` — additional arguments.
   - `timeout_ms: u64` — timeout in milliseconds (default: 5000).
   - `env: HashMap<String, String>` — extra environment variables.
   - `working_dir: Option<PathBuf>` — optional working directory override.

2. Implement a `new(config: &ExecProviderConfig) -> Self` constructor that
   copies fields from the deserialized config.

3. Register the file in `src/providers/mod.rs`.

### Considerations

- `ExecProvider` does not hold any persistent state beyond the config. It is cheap
  to construct and does not open connections, sockets, or file handles.
- Derive `Clone` if needed for test setups, but it is not required for production use.
- `ExecProvider` must be `Send + Sync` to satisfy the `SecretProvider` trait bound.
  All fields must also be `Send + Sync`. `HashMap`, `String`, `PathBuf`, and `u64`
  all satisfy this requirement.

---

## Step A2 — Implement Process Spawning

**Location:** `src/providers/exec.rs` — private helper method

Implement a private method `fn call(&self, request: &ExecRequest) -> Result<ExecResponse>`
that handles the full lifecycle of a single provider subprocess call.

### What to do

1. Use `std::process::Command` to configure the child process:
   - Set the executable path from `self.command`.
   - Add arguments from `self.args`.
   - Set `stdin(Stdio::piped())` and `stdout(Stdio::piped())`.
   - Set `stderr(Stdio::inherit())` so the provider's diagnostics appear in the terminal.
   - Apply `self.env` entries via `.env()` calls.
   - Apply `self.working_dir` via `.current_dir()` if set.

2. Call `.spawn()` to start the process. Map the `io::Error` to `DotenvzError::ProcessExec`.

3. Take ownership of the child's `stdin` and `stdout` handles.

4. Proceed to Step A3 (write request) and Step A4 (read response), subject to
   the timeout mechanism in Step A5.

### Considerations

- Do not use `Command::new("sh")` with `-c` to compose commands. The `command`
  field is used directly as the executable path. This prevents shell injection.
- Do not pass the secret value as a command-line argument. Command-line arguments
  are visible in `ps` output and process listings. Always use stdin.
- Capture stdout with `Stdio::piped()`. Reading an unpipped stdout handle would
  silently discard the response.

---

## Step A3 — Serialize the Request JSON

**Location:** `src/providers/exec.rs`

Define a `ExecRequest` struct that models the JSON request body and implement
serialization.

### What to do

1. Define `ExecRequest` with fields: `action`, `project`, `profile`, and optional
   `key` and `value`. Fields absent for a given action should serialize as omitted
   JSON properties (use `Option<String>` with `#[serde(skip_serializing_if = "Option::is_none")]`).

2. Add `serde_json` to `Cargo.toml` if not already present.

3. In the `call` method, serialize the `ExecRequest` to a JSON byte string using
   `serde_json::to_vec(&request)`.

4. Write the serialized bytes followed by a single `\n` byte to the child's stdin.

5. Close the stdin handle by dropping it. This sends EOF to the child, signalling
   that the request is complete.

### Considerations

- Writing to a closed pipe returns an `io::Error` with `BrokenPipe`. Map this
  to `DotenvzError::Provider("provider closed stdin prematurely")`.
- The `\n` terminator is required by the protocol but also aids line-buffered
  readers in the provider implementation. Always include it.
- The `value` field for `set` operations contains the raw secret string. Never
  log or print the serialized request bytes in production code paths.

---

## Step A4 — Read the Response Safely

**Location:** `src/providers/exec.rs`

Read the child's complete stdout output and parse it as a single JSON response.

### What to do

1. Define `ExecResponse`, `ExecSuccess`, and `ExecError` structs matching the
   protocol's response shape. The `ok` field is the discriminant.

2. Read the child stdout to a `String` using `io::Read::read_to_string`. Do not
   read line-by-line; read until EOF.

3. Trim trailing whitespace from the string.

4. Deserialize using `serde_json::from_str::<ExecResponse>(&stdout_text)`. Map a
   deserialization error to `DotenvzError::Provider("exec provider returned
   malformed JSON: <serde_json error message>")`.

5. If `response.ok` is `false`, read `response.error.code` and map it using
   the error mapping table in `docs/ARCHITECTURE_UPDATE.md`.

6. If `response.ok` is `true`, extract `value` or `keys` as appropriate for the
   calling trait method.

### Considerations

- An empty stdout string (the provider wrote nothing) should produce a clear error:
  `DotenvzError::Provider("exec provider produced no output")`.
- Do not parse stdout incrementally. The process must have exited (or been killed)
  before the response is used.
- Wait for the child process to exit after reading stdout:
  `child.wait()`. Check the exit status. A non-zero exit after valid JSON is still
  treated as the JSON error, not a crash, because the provider may exit non-zero
  to signal protocol-level errors. A non-zero exit with no JSON is a process crash.
- Do not interpret a non-zero exit code in isolation as the error type. Always
  attempt to parse stdout first.

---

## Step A5 — Handle Timeout

**Location:** `src/providers/exec.rs`

Impose an upper bound on the total time spent waiting for the provider process.

### What to do

1. Record the spawn time.
2. Spawn the child process (Step A2).
3. In a separate thread (or using `std::sync::mpsc` with a timeout receiver):
   - Perform the stdin write and stdout read in the child thread.
   - The main thread waits on the receiver with a deadline of `timeout_ms`.
4. If the deadline is reached before the result arrives:
   - Call `child.kill()` to send `SIGKILL` (Unix) / `TerminateProcess` (Windows).
   - Return `DotenvzError::Provider(format!("exec provider timed out after {}ms", self.timeout_ms))`.

### Considerations

- `child.kill()` is best-effort. The OS will clean up the process when the dotenvz
  process itself exits if `kill()` fails for any reason (e.g. the process already exited).
- After calling `kill()`, call `child.wait()` to reap the zombie process on Unix.
  Reaping is a hygiene requirement — it does not indicate success.
- `timeout_ms = 0` is the documented "disable timeout" sentinel. When `timeout_ms`
  is zero, skip the timeout mechanism and read stdout indefinitely. Add this check
  explicitly; do not pass zero to a sleep or timeout function.
- The timeout covers the entire operation from spawn to response receipt. It is
  not reset when stdin write completes. The provider must write its response
  within the original budget.

---

## Step A6 — Map Responses to Domain Errors

**Location:** `src/providers/exec.rs`

Define the authoritative error code mapping for exec provider responses.

### What to do

1. Write a function `map_exec_error(code: &str, message: &str) -> DotenvzError`
   that converts protocol error codes to `DotenvzError` variants:

   | Protocol `code`    | `DotenvzError` variant                             |
   |--------------------|----------------------------------------------------|
   | `"NOT_FOUND"`      | `DotenvzError::KeyNotFound`                        |
   | `"ACCESS_DENIED"`  | `DotenvzError::Provider(format!("access denied: {}", message))` |
   | `"INVALID_REQUEST"` | `DotenvzError::Provider(format!("invalid request: {}", message))` |
   | `"INTERNAL_ERROR"` | `DotenvzError::Provider(format!("provider error: {}", message))` |
   | `"TIMEOUT"`        | `DotenvzError::Provider(format!("provider timed out: {}", message))` |
   | _any other_        | `DotenvzError::Provider(format!("unknown error ({}): {}", code, message))` |

2. Call this function from the response handling logic in Step A4.

### Considerations

- `DotenvzError::KeyNotFound` is the only variant that carries no descriptive
  string beyond what `thiserror` formats. When mapping to it, discard the `message`
  from the provider, since the CLI will already format a "key not found" message
  using the key name from the command context.
- Never include secret values or partial values in the formatted error strings.

---

## Step A7 — Integrate with Provider Registry

**Location:** `src/main.rs` — `build_provider()`

Wire `ExecProvider` into the provider selection logic.

### What to do

1. In `build_provider()`, after the built-in provider check, look up the active
   provider name in `ctx.config.providers`.

2. When the matching `ProviderConfig` has `type = "exec"` (or `ProviderConfig::Exec`
   variant), construct `ExecProvider::new(&exec_cfg)` and return it as
   `Box<dyn SecretProvider>`.

3. Ensure the cloud provider path (introduced in Part B) and the exec provider
   path are handled in the same `match` arm structure so adding a future provider
   type requires touching only one place.

### Considerations

- `ExecProvider` is cross-platform (no `#[cfg(target_os)]` gate needed).
- Constructing `ExecProvider` must not fail. It does not validate that `command`
  exists on the filesystem at construction time — this is a deliberate trade-off
  to keep startup fast. The first `call()` attempt will surface a `ProcessExec`
  error if the binary is not found.

---

## Step A8 — Add Config Parsing for Exec Provider

**Location:** `src/config/model.rs`

Extend the config model to formally represent exec provider fields.

### What to do

1. Ensure `ProviderConfig` (whether as an enum variant or struct) includes all
   exec-specific fields:
   - `command: String`
   - `args: Option<Vec<String>>` (defaults to `[]`)
   - `timeout_ms: Option<u64>` (defaults to `5000`)
   - `env: Option<HashMap<String, String>>` (defaults to `{}`)
   - `working_dir: Option<String>`

2. Implement `Default` or `#[serde(default)]` for optional fields so that a
   minimal config (`type = "exec"`, `command = "..."`) parses without errors.

3. Extend `validate()` to enforce that `command` is non-empty when `type = "exec"`.

4. Add `"exec"` to `KNOWN_PROVIDERS` if it is not already present.

### Considerations

- `args` defaults to an empty vector. A missing `args` key in TOML is semantically
  identical to `args = []`.
- `working_dir` is stored as a `String` in the config model (mirrors how other path
  fields are handled). Conversion to `PathBuf` happens inside `ExecProvider::new()`.
- Validation must detect a missing `command` field and produce a `ConfigParse` error
  with the provider alias name included in the message for actionable diagnostics.

---

## Step A9 — Add Tests

**Location:** `tests/integration_test.rs` and inline `#[cfg(test)]` modules

See `docs/TEST_PLAN.md`, Section 3 (Exec Provider Tests) for the full test matrix.

### Quick checklist

- Unit tests for request serialization (all five action types).
- Unit tests for response deserialization (success and each error code).
- Unit tests for the error code mapping function.
- Integration test: a real subprocess (a shell script that echoes a known response)
  exercising the full `ExecProvider::get_secret` path.
- Integration test: timeout behavior (provider that sleeps longer than `timeout_ms`).
- Integration test: malformed JSON response.
- Integration test: provider exits non-zero with no output.

---

## Part B — Cloud Providers

These steps implement the cloud provider backends (AWS, GCP, Azure). They build on
the config model changes introduced in Part A.

---

## Step B1 — Define Provider Type Identifiers

**Location:** `src/config/model.rs`

Add a typed representation for the set of supported provider types.

### What to do

1. Create a `ProviderType` enum (or equivalent) covering:
   - `Exec`
   - `AwsSecretsManager`
   - `GcpSecretManager`
   - `AzureKeyVault`

2. Implement `serde::Deserialize` for this enum using the canonical string values
   (`"aws-secrets-manager"`, etc.) as the discriminant.

3. Use this enum in the updated `ProviderConfig` (Step B2) and in `validate()`.

4. Update `KNOWN_PROVIDERS` to list the new type strings so validation messages
   remain accurate.

### Considerations

- The enum should implement `Display` for use in error messages.
- Unknown `type` strings in TOML should produce a clear `ConfigParse` error at
  config load time, not at provider construction time.

---

## Step B2 — Redesign `ProviderConfig`

**Location:** `src/config/model.rs`

The current `ProviderConfig` struct is shaped around `exec` providers. It needs to
accommodate the distinct field sets for each cloud provider.

### What to do

Replace the single `ProviderConfig` struct with an enum whose variants hold
only the fields relevant to each provider type:

```
ProviderConfig::Exec   → existing fields (command, args, timeout_ms, env, working_dir)
ProviderConfig::Aws    → region: String, prefix: Option<String>
ProviderConfig::Gcp    → project_id: String, prefix: Option<String>
ProviderConfig::Azure  → vault_url: String, prefix: Option<String>
```

### TOML deserialization

TOML's `[providers.<name>]` sections are flat. The `type` field acts as the
discriminant. Use serde's `#[serde(tag = "type")]` or a custom `Deserialize`
implementation to route each entry to the correct variant based on its `type` value.

### Considerations

- Existing `.dotenvz.toml` files using `exec` providers must continue to parse
  without any changes (backwards compatibility).
- Provider configs that are missing required fields (e.g. `region` for AWS) should
  produce a `ConfigParse` error with the provider name and missing field.
- `validate()` must check required fields for each variant.

---

## Step B3 — Add Cargo.toml Dependencies

**Location:** `Cargo.toml`

Cloud SDKs are large. Gate them behind Cargo features to avoid inflating binary
size for users who only use local providers.

### What to add

Create three optional feature flags:

```
feature "aws"   → pulls in aws-sdk-secretsmanager (+ tokio runtime dep)
feature "gcp"   → pulls in google-cloud-secretmanager (or gcp_auth + reqwest)
feature "azure" → pulls in azure_security_keyvault_secrets (+ tokio)
```

A convenience feature `"cloud"` enables all three.

### Considerations

- `tokio` with the `rt` feature is required for blocking on async SDK calls
  using `Runtime::block_on`. Add it as an optional dependency activated by
  any cloud feature.
- Use `[features]` not `[target.'cfg(...)'.dependencies]` — cloud providers are
  cross-platform.
- Review each SDK's minimum supported Rust version (MSRV) to ensure compatibility.
- Keep the default feature set empty so `cargo build` without `--features` produces
  a binary identical to the current one.

---

## Step B4 — Create Cloud Provider Structs

**Location:** `src/providers/`

Create one file per cloud provider:

- `src/providers/aws_secrets_manager.rs`
- `src/providers/gcp_secret_manager.rs`
- `src/providers/azure_key_vault.rs`

### What each file contains

Each file defines a struct (e.g. `AwsSecretsManagerProvider`) that:

1. Holds the deserialized provider config (region/project_id/vault_url and prefix).
2. Holds an SDK client instance, constructed in `new()`.
3. Implements `SecretProvider`.

### Implementing `get_secret`

`get_secret(project, profile, key)`:

1. Construct the full secret name: `<prefix>/<key>` (or `<key>` when no prefix).
2. Call the SDK's single-secret retrieve operation.
3. Extract the string value from the response.
4. Map SDK errors to `DotenvzError` variants (see error mapping table in ARCHITECTURE_UPDATE.md).
5. Return the string value.

`project` and `profile` are available but are not used for cloud providers in the MVP
(isolation is managed via `prefix`). They may be used for future key derivation schemes.

### Implementing `list_secrets`

`list_secrets(project, profile)`:

1. Determine the set of keys to fetch. In the MVP, this requires a key manifest
   (see Step 5). Without a manifest, enumerate via prefix using the cloud provider's
   list API. See each provider spec for the enumeration mechanism.
2. For each key, call `get_secret(project, profile, key)`.
3. Collect results into `HashMap<String, String>`.
4. If any individual key fetch fails with `KeyNotFound`, skip it and continue (or
   surface the error — define a policy and document it).
5. Return the map.

### Implementing `set_secret` and `delete_secret`

Both return `DotenvzError::UnsupportedOperation` in the MVP.
Include a descriptive message explaining that secrets must be managed via the
cloud provider's console or IaC tooling.

### Async bridging

Each cloud provider implementation creates its own `tokio::runtime::Runtime` in
`new()` (using `Runtime::new()` or `Builder::new_current_thread().build()`).
All async SDK calls are executed with `self.runtime.block_on(async { … })`.

The runtime is stored in the struct alongside the SDK client. Because `Runtime`
is `Send + Sync`, the provider struct satisfies the `SecretProvider: Send + Sync`
bound.

### Feature gates

Gate each file behind the corresponding Cargo feature:

```rust
#[cfg(feature = "aws")]
// ... AwsSecretsManagerProvider implementation
```

Provide a stub implementation behind `#[cfg(not(feature = "aws"))]` that returns
`DotenvzError::Provider("dotenvz was built without AWS support. Enable the 'aws' feature.")`.
This ensures the binary always compiles but gives a clear error at runtime when
a cloud provider is configured but the feature was not compiled in.

---

## Step B5 — Extend Config Validation

**Location:** `src/config/model.rs` → `DotenvzConfig::validate()`

Extend the `validate()` method to:

1. Recognise `"aws-secrets-manager"`, `"gcp-secret-manager"`, `"azure-key-vault"`
   as valid `type` values (in addition to `"exec"`).
2. Enforce required fields per variant (see Step 2).
3. Validate `vault_url` starts with `https://`.
4. Validate that `[profiles.*].provider` names reference a declared `[providers.*]`
   key or a known built-in name.
5. Validate Azure key name compatibility: warn (or error) if expected keys contain
   characters that cannot be mapped to valid Key Vault secret names.

---

## Step B6 — Wire Into `build_provider()`

**Location:** `src/main.rs`

Extend `build_provider()` to resolve cloud providers:

### Resolution logic

```
1. Determine active provider name from context (profile override or top-level)

2. Is it a known built-in? → instantiate local OS provider (existing logic)

3. Is it a key in ctx.config.providers?
     → read ProviderConfig
     → match variant:
           ProviderConfig::Exec   → ExecProvider::new(cfg)
           ProviderConfig::Aws    → AwsSecretsManagerProvider::new(cfg)
           ProviderConfig::Gcp    → GcpSecretManagerProvider::new(cfg)
           ProviderConfig::Azure  → AzureKeyVaultProvider::new(cfg)

4. Neither → DotenvzError::ConfigParse (should have been caught in validate())
```

### ExecProvider migration

The current `build_provider()` does not handle `ExecProvider` — it is constructed
elsewhere. Consolidate all provider construction into `build_provider()` as part
of this change.

---

## Step B7 — Error and Key Mapping

**Location:** Inside each cloud provider file

Define a private helper function `map_sdk_error(err: SdkError) -> DotenvzError`
for each provider. This function:

1. Inspects the SDK error kind or HTTP status code.
2. Returns `DotenvzError::KeyNotFound` for resource-not-found errors.
3. Returns `DotenvzError::Provider("access denied: …")` for permission errors.
4. Returns `DotenvzError::Provider("…")` for all other errors.

Key name mapping:

- AWS and GCP: no character transformation needed for env var keys.
- Azure: implement `env_key_to_vault_name(key: &str) -> String` that replaces
  underscores with hyphens and lowercases the string, and
  `vault_name_to_env_key(name: &str) -> Result<String>` for the reverse mapping.

These helpers should be unit-tested in isolation (see TEST_PLAN.md).

---

## Step B8 — Add Integration Tests (Mocked)

**Location:** `tests/integration_test.rs` and a new `tests/cloud/` directory

Cloud provider integration tests must not make real network calls. Use two
strategies:

### Strategy A — Trait-level mock

Implement `SecretProvider` for a `MockCloudProvider` struct (extend the existing
`InMemoryProvider` in `src/providers/mock.rs` or create a cloud-specific mock).
Test the command handlers and env resolver through this mock without involving the
SDK at all.

### Strategy B — SDK-level mock server

For provider-level unit tests, start a local HTTP server that emulates the cloud
API response format. The provider is configured to point at `http://localhost:<port>`
instead of the real API endpoint. This tests the HTTP parsing, JSON extraction,
and error mapping logic in isolation.

Recommended libraries: `wiremock` or `mockito` for the mock HTTP server.

See TEST_PLAN.md for the full test matrix.

---

## Checklist Summary

| Step | Task                                    | File(s)                                   |
|------|-----------------------------------------|-------------------------------------------|
| 1    | Define `ProviderType` enum              | `src/config/model.rs`                     |
| 2    | Redesign `ProviderConfig` as enum       | `src/config/model.rs`                     |
| 3    | Add feature-gated SDK dependencies      | `Cargo.toml`                              |
| 4    | Create three provider structs           | `src/providers/aws_secrets_manager.rs`, `gcp_secret_manager.rs`, `azure_key_vault.rs` |
| 5    | Extend config validation                | `src/config/model.rs`                     |
| 6    | Extend `build_provider()`               | `src/main.rs`                             |
| 7    | Implement error and key mapping helpers | Each provider file                        |
| 8    | Add integration tests                   | `tests/`                                  |

---

## Security Considerations

- Never log, print, or include secret values in error messages.
- Do not store credentials in `ProviderConfig`. Credentials flow via the SDK's
  ambient resolution chain.
- The `--dry-run` path in commands must redact all secret values. Confirm that
  cloud providers produce `<redacted>` output, not real values.
- Tokio runtimes should be dropped as soon as provider use is complete to close
  the async I/O and thread pool.
- Audit any dependency brought in by the cloud SDKs for known CVEs before
  merging to the main branch.
- When building release binaries for distribution, build without cloud features
  by default so the binary has minimal attack surface.
