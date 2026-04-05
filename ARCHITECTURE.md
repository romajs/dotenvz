# Architecture — dotenvz

## Overview

`dotenvz` is a single Rust binary that:

1. Reads a per-project `.dotenvz.toml` config from the nearest ancestor directory.
2. Resolves the active profile (CLI flag → config default).
3. Fetches secrets from a `SecretProvider` (Apple Keychain in the MVP).
4. Either manages individual secrets or injects the full env map into a child process.

---

## Module responsibilities

| Module | Responsibility |
|---|---|
| `main.rs` | Entry point. Parses CLI, selects provider, dispatches to command handlers. |
| `cli.rs` | `clap`-derived CLI struct. Defines all subcommands and flags. `external_subcommand` captures unrecognised names for alias resolution. |
| `errors.rs` | Single `DotenvzError` enum (via `thiserror`). All modules return `errors::Result<T>`. |
| `config/model.rs` | `DotenvzConfig` struct — serde model for `.dotenvz.toml`. |
| `config/loader.rs` | `find_config_file` (walk-up search), `load_config`, `write_config`. |
| `core/project_context.rs` | `ProjectContext` — resolves cwd → config path → config → active profile. Passed into every command handler. |
| `core/command_resolver.rs` | Distinguishes built-in commands from `[commands]` aliases. |
| `core/env_resolver.rs` | Calls `provider.list_secrets()` and returns the env map for injection. |
| `core/process_runner.rs` | Spawns child processes with inherited env + injected secrets overlaid. |
| `providers/secret_provider.rs` | `SecretProvider` trait: `set_secret`, `get_secret`, `list_secrets`, `delete_secret`. |
| `providers/macos_keychain.rs` | macOS Keychain implementation (stub with TODO markers). |
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
    ├─ build_provider()
    │     └─ MacOsKeychainProvider::new()
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

`dotenvz exec -- cargo run` follows the same path but via `Commands::Exec`.

---

## Config flow

```
.dotenvz.toml
    │
    ├─ config/loader.rs: find_config_file() walk-up search
    ├─ config/loader.rs: load_config() → toml::from_str()
    └─ config/model.rs: DotenvzConfig

DotenvzConfig
    ├─ project         → Keychain namespace prefix
    ├─ provider        → selects SecretProvider impl
    ├─ default_profile → fallback when --profile not given
    ├─ import_file     → path used by `dotenvz import`
    ├─ schema_file     → (future) validation key list
    └─ commands        → alias name → shell command string
```

---

## Provider abstraction

```rust
pub trait SecretProvider: Send + Sync {
    fn set_secret(&self, project, profile, key, value) -> Result<()>;
    fn get_secret(&self, project, profile, key)        -> Result<String>;
    fn list_secrets(&self, project, profile)           -> Result<HashMap<String, String>>;
    fn delete_secret(&self, project, profile, key)     -> Result<()>;
}
```

The provider is constructed once in `main.rs` and passed as `&dyn SecretProvider`
into every command handler. Swapping backends (mock, cloud, etc.) requires only
changing `build_provider()`.

### Keychain secret naming

```
kSecAttrService = "dotenvz.<project>.<profile>"
kSecAttrAccount = "<key>"
kSecValueData   = "<value>" (UTF-8)
```

---

## Adding a new command

1. Add a variant to `Commands` in `src/cli.rs`.
2. Create `src/commands/<name>.rs` with a `pub fn run(...)` function.
3. Add `pub mod <name>;` to `src/commands/mod.rs`.
4. Add the match arm in `src/main.rs`.

---

## Adding a new provider

1. Add a file `src/providers/<name>.rs` implementing `SecretProvider`.
2. Add `pub mod <name>;` to `src/providers/mod.rs`.
3. Update `build_provider()` in `main.rs` to select the new impl based on
   `ctx.config.provider` string (or a new CLI flag).

---

## Future extensions (not MVP)

| Extension | Notes |
|---|---|
| In-memory mock provider | For unit / integration testing without Keychain access |
| Profile inheritance | A `staging` profile inherits `dev` defaults then overlays |
| Schema validation | Warn when a key in `schema_file` is missing from the provider |
| Shell hooks | `eval "$(dotenvz hook zsh)"` to auto-inject on `cd` |
| Cloud provider | Sync secrets to/from a remote store (e.g. AWS Secrets Manager) |
| Linux / Windows | Additional platform providers behind `cfg` gates |
| VS Code extension | Read context from dotenvz for launch configurations |
| `dotenvz run` profiles | Run multiple aliases in sequence with shared env |
