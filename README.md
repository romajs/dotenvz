# dotenvz

> Cross-platform CLI for secure environment injection via the OS secret store.

[![CI](https://github.com/romajs/dotenvz/actions/workflows/ci.yml/badge.svg)](https://github.com/romajs/dotenvz/actions/workflows/ci.yml) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE) [![Rust: stable](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org) [![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-informational.svg)]()

`dotenvz` is a Rust CLI that stores your project's environment variables in the
**native OS secret store** and injects them into child processes at runtime.
It has no runtime dependency on `.env` files — those are used only during
initial import/bootstrap.

| Platform | Secret backend |
|----------|----------------|
| macOS    | Apple Keychain (`security-framework`) |
| Linux    | Secret Service via D-Bus (`secret-service` crate) |
| Windows  | Credential Manager (`windows-sys` Win32 API) |

---

## Key principle

> **The OS secret store is the source of truth. `.env` files are for bootstrapping only.**

---

## Scope

- Rust-based CLI binary
- macOS, Linux, and Windows — each backed by the native secret store
- Per-project config via `.dotenvz.toml`
- `dotenvz init` auto-detects the current OS and writes the correct `provider` value
- Named command aliases with automatic env injection (`dotenvz dev`, `dotenvz build`)
- Explicit exec mode: `dotenvz exec -- <command> [args...]`
- One-time import from `.env` into the secret store (`dotenvz import`)

## Non-goals

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
| `project` | Unique identifier used as the secret namespace |
| `provider` | Backend — `"macos-keychain"`, `"linux-secret-service"`, or `"windows-credential"` (auto-set by `dotenvz init`) |
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

Secrets are isolated by project **and** profile, so `DATABASE_URL` can coexist
safely across `dev`, `staging`, and `production` on all platforms.

### macOS — Apple Keychain

| Keychain attribute | Value |
|---|---|
| Service (`kSecAttrService`) | `dotenvz.<project>.<profile>` |
| Account (`kSecAttrAccount`) | The env key (e.g. `DATABASE_URL`) |
| Password (`kSecValueData`) | The env value (UTF-8) |

### Linux — Secret Service (D-Bus / GNOME Keyring / KWallet)

Each secret is stored as an item in the default collection with these attributes:

| Item attribute | Value |
|---|---|
| `application` | `dotenvz` |
| `project` | The project name |
| `profile` | The active profile |
| `key` | The env key |
| Secret value | The env value (UTF-8) |

> **Note:** A running Secret Service daemon (e.g. `gnome-keyring-daemon` or
> `kwallet`) is required. dotenvz exits with a clear error if D-Bus is
> unavailable.

### Windows — Credential Manager

| Credential attribute | Value |
|---|---|
| Type | `CRED_TYPE_GENERIC` |
| TargetName | `dotenvz/<project>/<profile>/<key>` |
| CredentialBlob | The env value (UTF-8) |
| Persist | `CRED_PERSIST_LOCAL_MACHINE` |

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
  providers/           — SecretProvider trait + macOS / Linux / Windows impls
tests/
  fixtures/            — sample config and .env for tests
  integration_test.rs
```
