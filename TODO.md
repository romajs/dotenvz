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
- [x] `README.md`, `ARCHITECTURE.md`, `TODO.md`

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

## Future (post-MVP)

- [ ] Profile inheritance (`staging` inherits `dev` and overlays)
- [ ] Schema validation: warn on missing keys from `schema_file`
- [ ] `dotenvz diff --profile staging` — compare profiles
- [ ] Shell hook: `eval "$(dotenvz hook zsh)"`
- [ ] Cloud provider backend (AWS Secrets Manager, 1Password CLI, etc.)
- [ ] Linux support (e.g. libsecret / GNOME Keyring)
- [ ] Windows support (Credential Manager)
- [ ] Team sharing / sync
