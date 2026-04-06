# TODO — dotenvz

Practical implementation roadmap. Items are ordered roughly by dependency.

---

## Scaffold

- [x] Project structure and module layout
- [x] `Cargo.toml` with crate dependencies
- [x] CLI command wiring (`clap` + `external_subcommand` for aliases)
- [x] `DotenvzConfig` model and TOML loader
- [x] `SecretProvider` trait
- [x] `MacOsKeychainProvider` stub
- [x] `ProjectContext` resolver
- [x] `CommandResolver` (builtin vs alias)
- [x] `EnvResolver` shape
- [x] `ProcessRunner`
- [x] Command handler stubs: `init`, `import`, `set`, `get`, `list`, `rm`, `exec`
- [x] Test fixtures: `.dotenvz.toml`, `.env`
- [x] `README.md`
- [x] `TODO.md`
- [x] `docs/ARCHITECTURE.md`
- [x] `docs/OS_PROVIDERS_OVERVIEW.md`
- [x] `docs/PROVIDER_SPEC_MACOS.md`, `PROVIDER_SPEC_LINUX.md`, `PROVIDER_SPEC_WINDOWS.md`
- [x] `docs/CLOUD_PROVIDERS_OVERVIEW.md`
- [x] `docs/PROVIDER_SPEC_AWS.md`, `PROVIDER_SPEC_GCP.md`, `PROVIDER_SPEC_AZURE.md`
- [x] `docs/CUSTOM_PROVIDER_PROTOCOL.md`
- [x] `docs/DOTENVZ_CONFIG_EXTENSIONS.md`

---

## Phase 1 — Config and context

- [x] Unit tests for `config/loader.rs`: parse fixture, round-trip write/read
- [x] Unit tests for `core/command_resolver.rs`: builtin and alias cases
- [x] Unit tests for `core/project_context.rs`: walk-up directory search
- [x] Handle malformed `.dotenvz.toml` with clear user-facing error messages
- [x] Validate `provider` field value; error early on unknown providers

---

## Phase 2 — In-memory mock provider

- [x] Add `providers/mock.rs` implementing `SecretProvider` with a `HashMap` backend
- [x] Use mock provider in unit and integration tests
- [x] Test `set_secret` / `get_secret` / `list_secrets` / `delete_secret` via mock
- [x] Test `env_resolver::resolve_env` with mock provider
- [x] Test `process_runner::run_process` injects env correctly

---

## Phase 3 — macOS Keychain provider

- [x] Implement `MacOsKeychainProvider::set_secret` using `security-framework`
- [x] Implement `MacOsKeychainProvider::get_secret`
- [x] Implement `MacOsKeychainProvider::list_secrets` (key registry pattern)
- [x] Implement `MacOsKeychainProvider::delete_secret`
- [x] Handle Keychain permission / auth errors gracefully (`errSecItemNotFound`, `errSecDuplicateItem`)
- [ ] Manual smoke tests against the real macOS Keychain

---

## Phase 4 — `dotenvz import`

- [x] Finish `commands/import.rs` — wire to real Keychain provider
- [x] Skip keys with empty values (warn, don't fail)
- [x] Support `--dry-run` flag to preview what would be imported
- [x] Integration test using the fixture `.env` and mock provider

---

## Phase 5 — Env resolution and exec

- [x] Wire `commands/exec.rs` to real Keychain provider
- [x] Test alias execution end-to-end with mock provider
- [x] Handle `program not found` errors with a helpful message
- [x] Add `--dry-run` to `exec` / alias commands to print env without running

---

## Phase 6 — Polish and integration tests

- [x] Improve shell-word splitting in `process_runner::run_command_string` (`shell-words` crate)
- [x] Add `src/lib.rs` to expose internals for integration tests
- [x] Integration test: `init` → `import` → `list` → `exec` with mock provider
- [x] `dotenvz init --force` flag to allow overwriting existing config
- [ ] Better output formatting (consider `colored` or `indicatif` for UX)
- [ ] `dotenvz --version` and `dotenvz help <command>` polish

---

## Phase 7 — Cross-platform providers

- [x] Add `secret-service` (Linux) and `windows-sys` (Windows) platform-gated deps to `Cargo.toml`
- [x] Update `KNOWN_PROVIDERS` and `default_provider()` in `config/model.rs` to cover all three OSes
- [x] `dotenvz init` auto-detects OS and writes correct `provider` value via `cfg!()` macros
- [x] Implement `LinuxSecretServiceProvider` (`providers/linux_secret_service.rs`) using `secret_service::blocking`
- [x] Implement `WindowsCredentialProvider` (`providers/windows_credential.rs`) using `windows-sys` Win32 FFI
- [x] Non-native stubs return `DotenvzError::UnsupportedPlatform` on wrong OS
- [x] Expand GitHub Actions CI matrix to `[macos-latest, ubuntu-latest, windows-latest]`
- [ ] Manual smoke tests on Linux (Secret Service / GNOME Keyring)
- [ ] Manual smoke tests on Windows (Credential Manager)
- [ ] Manual smoke tests on macOS (real Keychain, not mock)

---

## Phase 8 — Custom exec provider

- [ ] Add `DotenvzError::UnsupportedOperation` variant to `errors.rs`
- [ ] Extend `ProviderConfig` in `config/model.rs` to an enum: `Exec { command, args, timeout_ms, env, working_dir }`
- [ ] Update `DotenvzConfig` to include a `providers: HashMap<String, ProviderConfig>` map
- [ ] Update `validate()` in `config/model.rs` to accept named providers
- [ ] Implement `ExecProvider` in `providers/exec.rs` — JSON stdin/stdout subprocess protocol
  - [ ] `get_secret` — spawn, `{"action":"get",…}`, parse response
  - [ ] `set_secret` — spawn, `{"action":"set",…}`, parse response
  - [ ] `list_secrets` — spawn list call, then N × get calls
  - [ ] `delete_secret` — spawn, `{"action":"rm",…}`, parse response
  - [ ] Timeout handling (`timeout_ms`)
  - [ ] Protocol error mapping → `DotenvzError` variants
- [ ] Export `ExecProvider` from `providers/mod.rs`
- [ ] Extend `build_provider()` in `main.rs` to resolve named providers from the config map
- [ ] Unit tests for `ExecProvider` using a mock echo executable
- [ ] Integration test: named exec provider round-trip via mock

---

## Phase 9 — Cloud providers

- [ ] Add optional feature-gated cloud SDK deps to `Cargo.toml`:
  `aws-sdk-secretsmanager`, `google-cloud-secretmanager`, `azure_security_keyvault_secrets`, `tokio`
- [ ] Extend `ProviderConfig` enum with `Aws { region, prefix }`, `Gcp { project_id, prefix }`, `Azure { vault_url, prefix }`
- [ ] Implement `AwsSecretsManagerProvider` (`providers/aws_secrets_manager.rs`)
  - [ ] `get_secret` — `GetSecretValue` API call
  - [ ] `list_secrets` — iterate declared keys, call `get_secret` per key
  - [ ] `set_secret` / `delete_secret` → `DotenvzError::UnsupportedOperation`
  - [ ] Bridge async SDK with `tokio::runtime::Runtime::block_on`
  - [ ] Map AWS SDK errors → `DotenvzError` variants
- [ ] Implement `GcpSecretManagerProvider` (`providers/gcp_secret_manager.rs`)
  - [ ] `get_secret` — `AccessSecretVersion` (`versions/latest`)
  - [ ] `list_secrets` — iterate declared keys, call `get_secret` per key
  - [ ] `set_secret` / `delete_secret` → `DotenvzError::UnsupportedOperation`
  - [ ] Bridge async SDK with `tokio::runtime::Runtime::block_on`
  - [ ] Map GCP gRPC errors → `DotenvzError` variants
- [ ] Implement `AzureKeyVaultProvider` (`providers/azure_key_vault.rs`)
  - [ ] `get_secret` — Key Vault Secrets REST API (`GET /secrets/<name>`)
  - [ ] `list_secrets` — iterate declared keys, apply underscore→hyphen+lowercase transform, call `get_secret` per key
  - [ ] `set_secret` / `delete_secret` → `DotenvzError::UnsupportedOperation`
  - [ ] Bridge async SDK with `tokio::runtime::Runtime::block_on`
  - [ ] Map Azure SDK errors → `DotenvzError` variants
- [ ] Export all three providers from `providers/mod.rs`
- [ ] Extend `build_provider()` in `main.rs` to handle `Aws`, `Gcp`, `Azure` config variants
- [ ] Integration tests using mock HTTP or injected stub for each cloud provider
- [ ] CI: test cloud provider compilation on all three OS targets

---

## Future (post-MVP)

- [ ] Profile inheritance (`staging` inherits `dev` and overlays)
- [ ] Schema validation: warn on missing keys from `schema_file`
- [ ] `dotenvz diff --profile staging` — compare profiles
- [ ] Shell hook: `eval "$(dotenvz hook zsh)"`
- [ ] Team sharing / sync
- [ ] Full async `SecretProvider` trait (`async fn` methods, `#[tokio::main]`)
- [ ] Parallel secret fetching in cloud and exec `list_secrets` (`tokio::join_all`)
- [ ] Secret value caching within a single process run
- [ ] Cloud write support — opt-in `set_secret` / `delete_secret` for cloud providers
- [ ] Better output formatting (`colored` or `indicatif`)
- [ ] `dotenvz --version` and `dotenvz help <command>` polish
