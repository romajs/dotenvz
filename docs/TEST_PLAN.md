# Test Plan — Exec Provider and Cloud Provider Support

## Scope

This document defines the tests required to validate exec provider and cloud
provider support in dotenvz. Tests are grouped by layer: unit, integration
(mocked), and end-to-end (real cloud — optional, flagged).

Existing tests must remain green. No test in this plan modifies the behaviour of
local providers.

---

## 1. Unit Tests — Exec Provider

Unit tests for the exec provider cover serialization, deserialization, and error
mapping. They do not spawn real subprocesses. They live in inline `#[cfg(test)]`
modules inside `src/providers/exec.rs` or `src/config/model.rs`.

### 1.1 Request serialization

**File:** `src/providers/exec.rs` (inline `#[cfg(test)]` module)

| Test case | Description |
|-----------|-------------|
| `serialize_get_request` | Construct an `ExecRequest` with `action = "get"`, `project`, `profile`, `key`. Serialize to JSON. Assert that `"action":"get"`, `"key":"..."` are present and `"value"` is absent. |
| `serialize_set_request` | `action = "set"` with `key` and `value`. Assert all five fields are present in the JSON output. |
| `serialize_list_request` | `action = "list"`, no `key` or `value`. Assert `key` and `value` fields are absent from the JSON output. |
| `serialize_rm_request` | `action = "rm"` with `key`. Assert `"value"` is absent. |
| `serialize_health_request` | `action = "health"`. Assert only `action`, `project`, `profile` are present. |
| `serialize_newline_appended` | The serialized bytes followed by a `\n` write produces exactly `<json_bytes>\n`. |

### 1.2 Response deserialization

**File:** `src/providers/exec.rs` (inline `#[cfg(test)]` module)

| Test case | Description |
|-----------|-------------|
| `deserialize_get_success` | Parse `{"ok":true,"value":"secret-value"}`. Assert `ok = true`, `value = "secret-value"`. |
| `deserialize_list_success` | Parse `{"ok":true,"keys":["A","B","C"]}`. Assert `ok = true`, `keys = ["A","B","C"]`. |
| `deserialize_empty_list` | Parse `{"ok":true,"keys":[]}`. Assert `keys` is an empty vector. |
| `deserialize_set_success` | Parse `{"ok":true}`. Assert `ok = true`, `value` is absent/null. |
| `deserialize_error_not_found` | Parse `{"ok":false,"error":{"code":"NOT_FOUND","message":"..."}}`. Assert `ok = false`, `code = "NOT_FOUND"`. |
| `deserialize_error_access_denied` | Parse error response with `"code":"ACCESS_DENIED"`. Assert code and message extracted correctly. |
| `deserialize_error_invalid_request` | Parse error response with `"code":"INVALID_REQUEST"`. |
| `deserialize_error_internal_error` | Parse error response with `"code":"INTERNAL_ERROR"`. |
| `deserialize_error_timeout` | Parse error response with `"code":"TIMEOUT"`. |
| `deserialize_error_unknown_code` | Parse error response with an unrecognised code (e.g. `"CUSTOM_CODE"`). Deserialization must succeed; mapping must produce `INTERNAL_ERROR`. |
| `deserialize_malformed_json` | Pass `"not json"` to the deserializer. Assert a `ConfigParse` or `Provider` error is returned. |
| `deserialize_empty_string` | Pass an empty string. Assert `Provider("exec provider produced no output")` error. |
| `deserialize_trailing_whitespace` | Pass `{"ok":true,"value":"v"}\n\n `. Assert trim is applied and parse succeeds. |

### 1.3 Error code mapping

**File:** `src/providers/exec.rs` (inline `#[cfg(test)]` module)

| Test case | Description |
|-----------|-------------|
| `map_not_found_to_key_not_found` | `map_exec_error("NOT_FOUND", "...")` returns `DotenvzError::KeyNotFound`. |
| `map_access_denied_to_provider` | `map_exec_error("ACCESS_DENIED", "msg")` returns `DotenvzError::Provider` containing `"access denied"`. |
| `map_invalid_request_to_provider` | `map_exec_error("INVALID_REQUEST", "msg")` returns `DotenvzError::Provider` containing `"invalid request"`. |
| `map_internal_error_to_provider` | `map_exec_error("INTERNAL_ERROR", "msg")` returns `DotenvzError::Provider` containing `"provider error"`. |
| `map_timeout_to_provider` | `map_exec_error("TIMEOUT", "msg")` returns `DotenvzError::Provider` containing `"timed out"`. |
| `map_unknown_code_to_provider` | `map_exec_error("BOGUS_CODE", "msg")` returns `DotenvzError::Provider` containing `"BOGUS_CODE"`. |
| `error_message_never_contains_secret` | The formatted error strings for access-denied and internal-error responses do not contain the `value` field from a concurrent `set` request. (Static assertion via test fixture.) |

### 1.4 Exec provider config parsing

**File:** `src/config/model.rs` (inline `#[cfg(test)]` module)

| Test case | Description |
|-----------|-------------|
| `parse_exec_provider_minimal` | TOML with only `type = "exec"` and `command = "/path/to/bin"`. Assert defaults: `args = []`, `timeout_ms = 5000`, `env = {}`, `working_dir = None`. |
| `parse_exec_provider_full` | TOML with all fields set. Assert each field is parsed as configured. |
| `parse_exec_provider_missing_command` | TOML with `type = "exec"` but no `command`. Assert `validate()` returns a `ConfigParse` error naming the provider alias. |
| `parse_exec_provider_empty_command` | `command = ""` — empty string. Assert `validate()` returns `ConfigParse`. |
| `parse_exec_provider_args_order` | `args = ["--a", "--b", "--c"]`. Assert order is preserved. |
| `parse_exec_timeout_zero` | `timeout_ms = 0`. Assert parsed as `0` (timeout disabled), no validation error. |
| `parse_exec_env_table` | `[providers.custom.env]` with multiple entries. Assert all entries appear in the config env map. |
| `parse_exec_working_dir` | `working_dir = "/tmp/workdir"`. Assert `working_dir` is `Some("/tmp/workdir")`. |

---

## 2. Integration Tests — Exec Provider

Integration tests spawn real subprocesses (small shell scripts checked in under
`tests/fixtures/exec-providers/`) and exercise the full `ExecProvider` code path.
They run on all platforms where the fixture scripts are available (Unix: `#!/bin/sh`).

Scripts must be committed with executable bit set (`chmod +x`). They are invoked
via the full `ExecProvider::get_secret` / `list_secrets` / `set_secret` /
`delete_secret` path — no protocol mocking.

### 2.1 Happy-path operations

**File:** `tests/integration_test.rs`

Fixture: `tests/fixtures/exec-providers/echo-provider.sh` — a shell script that
implements the full protocol for a fixed in-memory key/value set.

| Test case | Description |
|-----------|-------------|
| `exec_get_secret_success` | Call `ExecProvider::get_secret("proj","dev","KEY")`. Assert the returned value matches the fixture's known response. |
| `exec_list_secrets_success` | Call `list_secrets("proj","dev")`. Assert the returned `HashMap` contains the fixture's known keys. |
| `exec_set_secret_success` | Call `set_secret("proj","dev","KEY","val")`. Assert no error is returned. |
| `exec_delete_secret_success` | Call `delete_secret("proj","dev","KEY")`. Assert no error is returned. |
| `exec_get_returns_not_found` | Request a key the fixture does not know. Assert `DotenvzError::KeyNotFound`. |

### 2.2 Timeout behavior

Fixture: `tests/fixtures/exec-providers/slow-provider.sh` — a shell script that
sleeps for 30 seconds before writing any output.

| Test case | Description |
|-----------|-------------|
| `exec_timeout_kills_process` | Configure `timeout_ms = 200`. Call `get_secret`. Assert `DotenvzError::Provider` with "timed out" in the message. Assert the call returns within approximately `timeout_ms` (allow 3x for CI). |
| `exec_timeout_zero_disables_timeout` | Configure `timeout_ms = 0`. Use the echo-provider fixture. Assert success — the provider completes and no timeout is applied. |

### 2.3 Malformed output

Fixture: `tests/fixtures/exec-providers/garbage-provider.sh` — writes `not json\n`
to stdout and exits 0.

| Test case | Description |
|-----------|-------------|
| `exec_malformed_json_error` | Call any method on the garbage provider. Assert `DotenvzError::Provider` containing "malformed JSON". |
| `exec_empty_stdout_error` | A provider that writes nothing and exits 0. Assert `DotenvzError::Provider` containing "no output". |
| `exec_partial_json_error` | A provider that writes `{"ok":true,"val` (truncated) and exits. Assert `Provider("malformed JSON")`. |

### 2.4 Non-zero exit codes

| Test case | Description |
|-----------|-------------|
| `exec_nonzero_exit_with_valid_json` | Provider writes `{"ok":false,"error":{"code":"INTERNAL_ERROR","message":"crash"}}` and exits with code 1. Assert `DotenvzError::Provider("provider error: crash")` — exit code does not override JSON. |
| `exec_nonzero_exit_no_output` | Provider writes nothing and exits 1. Assert `DotenvzError::Provider("exec provider failed: exit code 1")`. |

### 2.5 Process spawn failures

| Test case | Description |
|-----------|-------------|
| `exec_missing_binary_error` | `command = "/nonexistent/binary"`. Assert `DotenvzError::ProcessExec`. |
| `exec_non_executable_binary_error` | `command` points to a file that exists but is not executable. Assert `DotenvzError::ProcessExec`. |

### 2.6 Extra environment variables

| Test case | Description |
|-----------|-------------|
| `exec_env_vars_passed_to_provider` | Fixture reads `DOTENVZ_TEST_VAR` from its environment and echoes it in the response `value`. Assert the value is what was configured in the `env` map. |

---

## 3. Unit Tests — Cloud Providers

Unit tests cover logic that can be verified without a running provider, SDK, or
network call. These tests live inside the relevant source files or in
`tests/unit/` if extracted.

### 3.1 Config parsing

**File:** `src/config/model.rs` (inline `#[cfg(test)]` module)

| Test case | Description |
|-----------|-------------|
| `parse_aws_provider_config` | Deserialise a TOML snippet with `type = "aws-secrets-manager"`, `region`, and `prefix`. Assert all fields are parsed correctly. |
| `parse_gcp_provider_config` | Deserialise a TOML snippet with `type = "gcp-secret-manager"` and `project_id`. |
| `parse_azure_provider_config` | Deserialise a TOML snippet with `type = "azure-key-vault"` and `vault_url`. |
| `parse_unknown_provider_type` | TOML with an unknown `type` value must produce a `ConfigParse` error. |
| `parse_aws_missing_region` | AWS config without `region` must fail `validate()` with a `ConfigParse` error. |
| `parse_gcp_missing_project_id` | GCP config without `project_id` must fail validation. |
| `parse_azure_missing_vault_url` | Azure config without `vault_url` must fail validation. |
| `parse_azure_invalid_vault_url` | Azure config with `vault_url = "http://..."` (non-HTTPS) must fail validation. |
| `parse_profile_with_cloud_provider` | A `[profiles.prod]` with `provider = "aws"` referencing a declared `[providers.aws]` block parses successfully. |
| `parse_profile_unknown_provider` | A profile referencing an undeclared provider name fails validation. |
| `parse_exec_provider_unchanged` | Existing `exec` provider TOML parses correctly after the `ProviderConfig` refactor (regression). |
| `prefix_optional_for_all_types` | All three cloud provider types successfully parse when `prefix` is omitted. |

### 3.2 Key name mapping logic

**File:** `src/providers/aws_secrets_manager.rs`, `gcp_secret_manager.rs`, `azure_key_vault.rs`

| Test case | Description |
|-----------|-------------|
| `aws_build_secret_name_with_prefix` | `prefix = "my-app/dev"`, key `DATABASE_URL` → `"my-app/dev/DATABASE_URL"`. |
| `aws_build_secret_name_no_prefix` | No prefix, key `API_KEY` → `"API_KEY"`. |
| `gcp_build_secret_name_with_prefix` | `prefix = "my-app-dev"`, key `PORT` → `"my-app-dev-PORT"`. |
| `gcp_build_secret_name_no_prefix` | No prefix, key `API_KEY` → `"API_KEY"`. |
| `azure_env_key_to_vault_name_underscore` | `DATABASE_URL` → `"database-url"`. |
| `azure_env_key_to_vault_name_with_prefix` | `prefix = "my-app-dev"`, key `DATABASE_URL` → `"my-app-dev-database-url"`. |
| `azure_vault_name_to_env_key_round_trip` | Converting to vault name and back produces the original key. |
| `azure_vault_name_invalid_chars` | A key that produces an invalid Azure secret name (non-alphanumeric, non-hyphen) triggers `ConfigParse` or `Provider` error. |

### 3.3 Error mapping

| Test case | Description |
|-----------|-------------|
| `aws_not_found_maps_to_key_not_found` | SDK `ResourceNotFoundException` maps to `DotenvzError::KeyNotFound`. |
| `aws_access_denied_maps_to_provider` | SDK `AccessDeniedException` maps to `DotenvzError::Provider("access denied …")`. |
| `gcp_not_found_maps_to_key_not_found` | gRPC `NOT_FOUND` maps to `DotenvzError::KeyNotFound`. |
| `gcp_permission_denied_maps_to_provider` | gRPC `PERMISSION_DENIED` maps to `DotenvzError::Provider`. |
| `azure_404_maps_to_key_not_found` | HTTP 404 maps to `DotenvzError::KeyNotFound`. |
| `azure_403_maps_to_provider` | HTTP 403 maps to `DotenvzError::Provider("access denied …")`. |
| `error_message_does_not_contain_secret_value` | Error messages from any provider contain only SDK-level metadata, never the secret value string. |

### 3.4 `UnsupportedOperation` for write methods (MVP)

| Test case | Description |
|-----------|-------------|
| `aws_set_secret_returns_unsupported` | `set_secret` on `AwsSecretsManagerProvider` returns `DotenvzError::UnsupportedOperation`. |
| `aws_delete_secret_returns_unsupported` | Same for `delete_secret`. |
| `gcp_set_secret_returns_unsupported` | Same for GCP. |
| `gcp_delete_secret_returns_unsupported` | Same for GCP. |
| `azure_set_secret_returns_unsupported` | Same for Azure. |
| `azure_delete_secret_returns_unsupported` | Same for Azure. |

---

## 4. Integration Tests — Cloud Providers (Mocked Network)

Integration tests exercise the full dotenvz flow — config loading → provider
construction → secret retrieval → env injection — without making real cloud API
calls. A mock HTTP server stands in for the cloud endpoint.

### Test infrastructure

Use `wiremock` (or `mockito`) to start a local HTTP server per test. Point the
cloud provider SDK at the local server by setting the endpoint URL override
available in each SDK's client builder.

For the SDK endpoint override to work, the provider struct must accept an optional
`endpoint_url: Option<String>` in its config or constructor, used only in tests
(feature-gated behind `#[cfg(test)]` or a dedicated `"test-utils"` feature).

### 4.1 `get_secret` — success paths

| Test case | Description |
|-----------|-------------|
| `aws_get_secret_returns_value` | Mock server returns a valid `GetSecretValue` response. Provider returns the expected string. |
| `gcp_get_secret_returns_value` | Mock server returns a valid `AccessSecretVersion` response with base64-encoded payload. Provider decodes and returns the string. |
| `azure_get_secret_returns_value` | Mock server returns a valid Get Secret response JSON. Provider returns the `value` field. |

### 4.2 `get_secret` — error paths

| Test case | Description |
|-----------|-------------|
| `aws_get_secret_not_found` | Mock returns 400 `ResourceNotFoundException`. `get_secret` returns `DotenvzError::KeyNotFound`. |
| `aws_get_secret_access_denied` | Mock returns 400 `AccessDeniedException`. Returns `DotenvzError::Provider`. |
| `gcp_get_secret_not_found` | Mock returns gRPC `NOT_FOUND`. Returns `DotenvzError::KeyNotFound`. |
| `azure_get_secret_not_found` | Mock returns HTTP 404. Returns `DotenvzError::KeyNotFound`. |
| `any_provider_network_error` | Mock server is not started / connection refused. Returns `DotenvzError::Provider` with a network error message. |

### 4.3 `list_secrets`

| Test case | Description |
|-----------|-------------|
| `aws_list_secrets_multiple_keys` | Mock returns success for each of N keys. `list_secrets` returns a map of all N entries. |
| `gcp_list_secrets_multiple_keys` | Same for GCP. |
| `azure_list_secrets_multiple_keys` | Same for Azure. |
| `list_secrets_partial_failure` | One key returns `NOT_FOUND`, others succeed. Document the expected behaviour (skip vs. error). |

### 4.4 Full exec flow with cloud provider

These tests replicate the existing `exec` integration test structure but with a
cloud provider mock in place of the local keychain.

| Test case | Description |
|-----------|-------------|
| `exec_with_aws_provider_injects_env` | `.dotenvz.toml` with `[providers.aws]` config; mock AWS server returns secrets; `dotenvz exec -- env` output contains expected vars. |
| `exec_with_gcp_provider_injects_env` | Same for GCP. |
| `exec_with_azure_provider_injects_env` | Same for Azure. |
| `exec_dry_run_redacts_cloud_secrets` | `--dry-run` shows key list with `<redacted>`, not real values. |

### 4.5 Config validation end-to-end

| Test case | Description |
|-----------|-------------|
| `invalid_config_exits_with_error` | A `.dotenvz.toml` with missing `region` causes a process exit with error code 1 and a `ConfigParse` message. No network call is made. |
| `unknown_provider_type_exits_with_error` | Same: unknown `type` produces a clear error at config load. |

---

## 5. End-to-End Tests (Real Cloud — Optional, Flagged)

E2E tests connect to actual cloud accounts. They are:

- **Not run in CI by default.** They require secrets, IAM roles, and network access.
- Enabled by an environment variable flag: `DOTENVZ_E2E_CLOUD=1`.
- Gated behind a Cargo feature `"e2e"` to ensure they are excluded from normal builds.

### 5.1 AWS end-to-end

Require: `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` or an assumed IAM role.

| Test case | Description |
|-----------|-------------|
| `aws_e2e_get_secret` | Retrieve a pre-provisioned test secret from a real AWS Secrets Manager instance. |
| `aws_e2e_list_secrets` | List all secrets for a test prefix; confirm expected keys are present. |
| `aws_e2e_key_not_found` | Request a key that does not exist; confirm `KeyNotFound` error. |

### 5.2 GCP end-to-end

Require: `GOOGLE_APPLICATION_CREDENTIALS` or Workload Identity.

| Test case | Description |
|-----------|-------------|
| `gcp_e2e_get_secret` | Retrieve a pre-provisioned secret version from a real GCP project. |
| `gcp_e2e_latest_version` | Confirm that `latest` resolves to the expected version. |

### 5.3 Azure end-to-end

Require: `AZURE_CLIENT_ID` + `AZURE_CLIENT_SECRET` + `AZURE_TENANT_ID` or Managed Identity.

| Test case | Description |
|-----------|-------------|
| `azure_e2e_get_secret` | Retrieve a pre-provisioned secret from a real Key Vault. |
| `azure_e2e_name_mapping` | Confirm that underscore-to-hyphen mapping resolves to the correct vault secret. |

### E2E test setup documentation

For each cloud provider, document:

- How to provision the test secret (console or CLI command).
- Required IAM/RBAC role assignments.
- How to tear down test secrets after the run.
- Estimated cost per test run.

---

## 6. Regression Tests

| Test case | Description |
|-----------|-------------|
| `local_macos_provider_unaffected` | Existing macOS Keychain tests pass after config model refactor. |
| `local_linux_provider_unaffected` | Existing Linux Secret Service tests pass. |
| `local_windows_provider_unaffected` | Existing Windows Credential Manager tests pass. |
| `exec_provider_unaffected` | Existing exec provider integration tests pass. |
| `mock_provider_unaffected` | All tests using `InMemoryProvider` pass unchanged. |

These tests should be run as part of the standard `cargo test` suite on every PR.

---

## 7. Coverage Targets

| Layer                    | Target line coverage |
|--------------------------|----------------------|
| Config parsing (model.rs) | ≥ 90%               |
| Key mapping helpers       | 100%                |
| Error mapping helpers     | 100%                |
| Provider `get_secret`     | ≥ 80% (via mocked)  |
| Provider `list_secrets`   | ≥ 80% (via mocked)  |

Measure coverage using `cargo llvm-cov` or `cargo tarpaulin`.

---

## 8. Test Execution Commands

```sh
# Run all unit and integration tests (no real cloud calls)
cargo test

# Run only cloud provider unit tests
cargo test --test '*' cloud

# Run E2E tests (requires cloud credentials and e2e feature)
DOTENVZ_E2E_CLOUD=1 cargo test --features e2e

# Run with coverage (requires cargo-llvm-cov)
cargo llvm-cov --all-features
```
