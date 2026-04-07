# dotenvz

> Cross-platform CLI for secure environment injection via the OS secret store.

[![CI](https://github.com/romajs/dotenvz/actions/workflows/ci.yml/badge.svg)](https://github.com/romajs/dotenvz/actions/workflows/ci.yml) [![License: Proprietary](https://img.shields.io/badge/license-Proprietary-red.svg)](LICENSE) [![Rust: stable](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org) [![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-informational.svg)]()

`dotenvz` is a Rust CLI that stores your project's environment variables in a
secret backend and injects them into child processes at runtime.
It has no runtime dependency on `.env` files — those are used only during
initial import/bootstrap.

**OS providers** (default, zero-config):

| Platform | Secret backend |
|----------|----------------|
| macOS    | iCloud Keychain / Passwords.app (`macos-passwords`, default) or local-only login Keychain (`macos-keychain`) |
| Linux    | Secret Service via D-Bus (`secret-service` crate) |
| Windows  | Credential Manager (`windows-sys` Win32 API) |

<!--
**Cloud providers** (read-only, declared in `.dotenvz.toml`):

| Provider key              | Backend                        |
|---------------------------|--------------------------------|
| `aws-secrets-manager`     | AWS Secrets Manager            |
| `gcp-secret-manager`      | Google Cloud Secret Manager    |
| `azure-key-vault`         | Azure Key Vault                |

**Custom providers** — any executable that speaks the dotenvz JSON protocol
(`type = "exec"` in `.dotenvz.toml`).
-->

---

## Key principle

> **The OS secret store is the source of truth. `.env` files are for bootstrapping only.**

---

## Scope

- Rust-based CLI binary
- macOS, Linux, and Windows — each backed by the native secret store
<!--- Cloud provider backends: AWS Secrets Manager, GCP Secret Manager, Azure Key Vault -->
<!--- Custom exec providers — delegate to any local executable over JSON stdin/stdout -->
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
- Team sharing / syncing secrets between developers
- Encrypted file storage as a runtime secret store
<!--- Writing secrets to cloud providers (cloud backends are read-only in the current release) -->

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
provider = "macos-passwords"
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
| `provider` | Secret backend — `"macos-passwords"` (macOS default, iCloud Keychain / Passwords.app with local fallback), `"macos-keychain"` (local-only login Keychain), `"linux-secret-service"`, or `"windows-credential"` (auto-set by `dotenvz init`) |
| `default_profile` | Profile used when `--profile` is not specified |
| `schema_file` | Path to a file listing expected keys (future validation) |
| `import_file` | `.env` file used by `dotenvz import` |
| `[commands]` | Named aliases: `dotenvz <name>` → command string with env injected |

<!--
### Using a cloud provider

Declare a named provider under `[providers.<name>]` and reference it from a profile:

```toml
[providers.aws]
type   = "aws-secrets-manager"
region = "us-east-1"
prefix = "my-app/dev"

[profiles.dev]
provider = "aws"
```

Cloud providers use **ambient credentials** (IAM role, ADC, Managed Identity) —
no credentials are stored in `.dotenvz.toml`. See [`docs/CLOUD_PROVIDERS_OVERVIEW.md`](docs/CLOUD_PROVIDERS_OVERVIEW.md).

### Using a custom exec provider

```toml
[providers.vault]
type       = "exec"
command    = "/usr/local/bin/my-vault-bridge"
timeout_ms = 5000

[profiles.dev]
provider = "vault"
```

See [`docs/CUSTOM_PROVIDER_PROTOCOL.md`](docs/CUSTOM_PROVIDER_PROTOCOL.md) for the JSON wire protocol.
-->

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

### macOS — iCloud Keychain / Passwords.app (`macos-passwords`)

Secrets are stored as **synchronizable** Generic Password items, making them visible
in the macOS **Passwords** app and synced via iCloud Keychain across your devices.
If iCloud is unavailable the provider falls back to the local login Keychain silently.

| Keychain attribute | Value |
|---|---|
| Service (`kSecAttrService`) | `dotenvz.<project>.<profile>` |
| Account (`kSecAttrAccount`) | The env key (e.g. `DATABASE_URL`) |
| Password (`kSecValueData`) | The env value (UTF-8) |
| Synchronizable (`kSecAttrSynchronizable`) | `true` (iCloud sync) |

### macOS — Local Keychain only (`macos-keychain`)

Set `provider = "macos-keychain"` to store in the local login Keychain only (no iCloud sync).
Layout is identical to `macos-passwords` minus the synchronizable flag.

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
  providers/
    secret_provider.rs    — SecretProvider trait
    macos_passwords.rs    — macOS iCloud Keychain / Passwords.app (default)
    macos_keychain.rs     — macOS local login Keychain
    linux_secret_service.rs — Linux Secret Service (D-Bus)
    windows_credential.rs   — Windows Credential Manager (Win32)
    mock.rs               — in-memory backend for tests
<!--
    exec.rs            — custom exec provider (JSON subprocess protocol)
    aws_secrets_manager.rs  — AWS Secrets Manager
    gcp_secret_manager.rs   — Google Cloud Secret Manager
    azure_key_vault.rs      — Azure Key Vault
-->
tests/
  fixtures/            — sample config and .env for tests
  integration_test.rs
```

---

## Documentation

| Document | Description |
|----------|-------------|
| [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) | Full architecture, module map, execution flow, provider categories |
| [`docs/OS_PROVIDERS_OVERVIEW.md`](docs/OS_PROVIDERS_OVERVIEW.md) | Overview of all three OS providers and how they compare |
| [`docs/PROVIDER_SPEC_MACOS.md`](docs/PROVIDER_SPEC_MACOS.md) | macOS Keychain provider — storage layout, operation flow, error handling |
| [`docs/PROVIDER_SPEC_LINUX.md`](docs/PROVIDER_SPEC_LINUX.md) | Linux Secret Service provider — D-Bus, GNOME Keyring, KWallet |
| [`docs/PROVIDER_SPEC_WINDOWS.md`](docs/PROVIDER_SPEC_WINDOWS.md) | Windows Credential Manager provider — Win32 Cred* API |
| [`docs/IMPLEMENTATION_GUIDE.md`](docs/IMPLEMENTATION_GUIDE.md) | Implementation notes, security considerations, testing approach |
| [`docs/TEST_PLAN.md`](docs/TEST_PLAN.md) | Test plan covering unit, integration, and platform smoke tests |

<!--
| [`docs/CLOUD_PROVIDERS_OVERVIEW.md`](docs/CLOUD_PROVIDERS_OVERVIEW.md) | Cloud providers overview — auth model, read-only design, execution flow |
| [`docs/PROVIDER_SPEC_AWS.md`](docs/PROVIDER_SPEC_AWS.md) | AWS Secrets Manager provider spec |
| [`docs/PROVIDER_SPEC_GCP.md`](docs/PROVIDER_SPEC_GCP.md) | Google Cloud Secret Manager provider spec |
| [`docs/PROVIDER_SPEC_AZURE.md`](docs/PROVIDER_SPEC_AZURE.md) | Azure Key Vault provider spec |
| [`docs/CUSTOM_PROVIDER_PROTOCOL.md`](docs/CUSTOM_PROVIDER_PROTOCOL.md) | Exec provider JSON wire protocol for custom backends |
| [`docs/DOTENVZ_CONFIG_EXTENSIONS.md`](docs/DOTENVZ_CONFIG_EXTENSIONS.md) | Config reference for exec and cloud provider declarations |
-->
