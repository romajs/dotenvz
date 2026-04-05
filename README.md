# dotenvz

> Universal CLI for secure environment injection via Apple Keychain.

`dotenvz` is a Rust CLI that stores your project's environment variables in the
**macOS Keychain** and injects them into child processes at runtime. It is not a
Node.js library and has no runtime dependency on `.env` files — those are used
only during initial import/bootstrap.

---

## Key principle

> **The macOS Keychain is the source of truth. `.env` files are for bootstrapping only.**

---

## MVP Scope

- Rust-based CLI binary
- macOS only — Apple Keychain as the secret store
- Per-project config via `.dotenvz.toml`
- Named command aliases with automatic env injection (`dotenvz dev`, `dotenvz build`)
- Explicit exec mode: `dotenvz exec -- <command> [args...]`
- One-time import from `.env` into Keychain (`dotenvz import`)

## Non-goals (MVP)

- Linux / Windows support
- Shell hooks (`.bashrc`, `.zshrc` integration)
- Node.js runtime import integration
- Docker secret bridge
- Biometric / auth customization
- VS Code extension integration
- Cloud sync / team sharing
- Encrypted file storage as a runtime secret store

---

## Installation

```bash
cargo install --path .
```

---

## Configuration

Place a `.dotenvz.toml` in your project root. Run `dotenvz init` to scaffold one:

```toml
project = "my-app"
provider = "macos-keychain"
default_profile = "dev"
schema_file = ".env.example"
import_file = ".env"

[commands]
dev   = "next dev"
build = "next build"
start = "next start"
test  = "cargo test"
```

| Field | Description |
|---|---|
| `project` | Unique identifier used as the Keychain namespace |
| `provider` | Backend — currently only `"macos-keychain"` |
| `default_profile` | Profile used when `--profile` is not specified |
| `schema_file` | Path to a file listing expected keys (future validation) |
| `import_file` | `.env` file used by `dotenvz import` |
| `[commands]` | Named aliases: `dotenvz <name>` → command string with env injected |

---

## Commands

```bash
# Scaffold .dotenvz.toml
dotenvz init

# Import from .env into Keychain (one-time bootstrap)
dotenvz import
dotenvz import --file .env.staging

# Manage secrets
dotenvz set DATABASE_URL postgres://localhost/mydb
dotenvz get DATABASE_URL
dotenvz list
dotenvz rm DATABASE_URL

# Run a command with secrets injected
dotenvz exec -- next dev
dotenvz exec -- cargo run -- --port 8080

# Use a named alias from [commands]
dotenvz dev
dotenvz build
dotenvz test
```

Override the profile for any command:

```bash
dotenvz --profile production list
dotenvz --profile staging exec -- ./deploy.sh
```

---

## How secrets are stored

Secrets are stored as **Generic Password** items in the macOS login Keychain:

| Keychain attribute | Value |
|---|---|
| Service (`kSecAttrService`) | `dotenvz.<project>.<profile>` |
| Account (`kSecAttrAccount`) | The env key (e.g. `DATABASE_URL`) |
| Password (`kSecValueData`) | The env value |

This means secrets are isolated by project **and** profile, so `DATABASE_URL`
can coexist safely across `dev`, `staging`, and `production`.

---

## Project structure

```
src/
  main.rs              — entry point + provider wiring
  cli.rs               — clap command definitions
  errors.rs            — central error type
  commands/            — one file per CLI command
  config/              — .dotenvz.toml model + loader
  core/                — project context, resolver, process runner
  providers/           — SecretProvider trait + macOS Keychain impl
tests/
  fixtures/            — sample config and .env for tests
  integration_test.rs
```
