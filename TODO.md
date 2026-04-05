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

- [ ] Unit tests for `config/loader.rs`: parse fixture, round-trip write/read
- [ ] Unit tests for `core/command_resolver.rs`: builtin and alias cases
- [ ] Unit tests for `core/project_context.rs`: walk-up directory search
- [ ] Handle malformed `.dotenvz.toml` with clear user-facing error messages
- [ ] Validate `provider` field value; error early on unknown providers

---

## Phase 2 — In-memory mock provider

- [ ] Add `providers/mock.rs` implementing `SecretProvider` with a `HashMap` backend
- [ ] Use mock provider in unit and integration tests
- [ ] Test `set_secret` / `get_secret` / `list_secrets` / `delete_secret` via mock
- [ ] Test `env_resolver::resolve_env` with mock provider
- [ ] Test `process_runner::run_process` injects env correctly

---

## Phase 3 — macOS Keychain provider

- [ ] Implement `MacOsKeychainProvider::set_secret` using `security-framework`
- [ ] Implement `MacOsKeychainProvider::get_secret`
- [ ] Implement `MacOsKeychainProvider::list_secrets` (query by service prefix)
- [ ] Implement `MacOsKeychainProvider::delete_secret`
- [ ] Handle Keychain permission / auth errors gracefully
- [ ] Manual smoke tests against the real macOS Keychain

---

## Phase 4 — `dotenvz import`

- [ ] Finish `commands/import.rs` — wire to real Keychain provider
- [ ] Skip keys with empty values (warn, don't fail)
- [ ] Support `--dry-run` flag to preview what would be imported
- [ ] Integration test using the fixture `.env` and mock provider

---

## Phase 5 — Env resolution and exec

- [ ] Wire `commands/exec.rs` to real Keychain provider
- [ ] Test alias execution end-to-end with mock provider
- [ ] Handle `program not found` errors with a helpful message
- [ ] Add `--dry-run` to `exec` / alias commands to print env without running

---

## Phase 6 — Polish and integration tests

- [ ] Improve shell-word splitting in `process_runner::run_command_string`
  (consider the `shell-words` crate to handle quoted arguments)
- [ ] Add `src/lib.rs` to expose internals for integration tests
- [ ] Integration test: `init` → `import` → `list` → `exec` with mock provider
- [ ] Better output formatting (consider `colored` or `indicatif` for UX)
- [ ] `dotenvz --version` and `dotenvz help <command>` polish
- [ ] `dotenvz init --force` flag to allow overwriting existing config

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
