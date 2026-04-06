# dotenvz Config Extensions — Exec and Cloud Providers

## Overview

dotenvz supports two categories of configurable providers declared under
`[providers.<name>]` in `.dotenvz.toml`:

- **Exec provider** (`type = "exec"`) — delegates all secret operations to a
  local executable using a JSON stdin/stdout protocol.
- **Cloud providers** (`type = "aws-secrets-manager"`, `"gcp-secret-manager"`,
  `"azure-key-vault"`) — use managed cloud secret stores.

Everything else in the config file — `[profiles.*]`, `[commands]`, `project`,
`default_profile` — works identically to the existing local-provider behaviour.

---

## Exec Provider

### Minimal example

```toml
project         = "my-app"
default_profile = "dev"

[providers.custom]
type        = "exec"
command     = "/usr/local/bin/my-provider"
args        = ["--mode", "dotenvz"]
timeout_ms  = 5000

[profiles.dev]
provider = "custom"
```

### Full example with all optional fields

```toml
[providers.vault]
type        = "exec"
command     = "/opt/tools/vault-bridge"
args        = ["--serve", "--format", "dotenvz"]
timeout_ms  = 8000
working_dir = "/opt/tools"

[providers.vault.env]
VAULT_ADDR  = "https://vault.internal:8200"
VAULT_TOKEN = "s.exampletoken"
```

### Exec provider field reference

| Field         | Type            | Required | Default | Description |
|---------------|-----------------|----------|---------|-------------|
| `type`        | string          | yes      | —       | Must be `"exec"`. |
| `command`     | string          | yes      | —       | Absolute path to the provider executable. Relative paths are resolved from `working_dir` if set, otherwise from the directory containing `.dotenvz.toml`. **Prefer absolute paths.** |
| `args`        | array of string | no       | `[]`    | Arguments passed to the executable as separate argv entries. Shell metacharacters are NOT interpreted. |
| `timeout_ms`  | integer         | no       | `5000`  | Maximum milliseconds to wait for a response before killing the process. Set to `0` to disable (not recommended). |
| `env`         | table           | no       | `{}`    | Extra environment variables injected into the provider process in addition to the current environment. Useful for passing tokens or configuration that should not appear in `.dotenvz.toml` values verbatim (consider using `$ENV_VAR` references if supported). |
| `working_dir` | string          | no       | config file's directory | Working directory for the spawned process. |

### `command`

The executable is launched directly via `std::process::Command`, not through a
shell. This means:

- `command` must be a path to an actual executable file, not a shell built-in or alias.
- Shell operators (`&&`, `|`, `;`) in the value are passed literally to the binary,
  not executed.
- If you need a shell script, make the script itself executable and point `command`
  at the script file (e.g. `#!/usr/bin/env bash` shebang).

### `args`

Each element in the `args` array is a separate argument in the OS argument vector.
There is no word-splitting or glob expansion. The following two configurations are
NOT equivalent:

```toml
# Correct — two separate arguments
args = ["--mode", "dotenvz"]

# Wrong — single argument containing a space (passed literally to the binary)
args = ["--mode dotenvz"]
```

### `timeout_ms`

The timeout applies from the moment the process is spawned to the moment dotenvz
receives the complete response. It includes:

- Process startup time.
- Time to read the request from stdin.
- Time for the provider to compute the response.
- Time to write the response to stdout.

Design your provider to respond well within this window. A sensible upper bound is
`10000` ms (10 seconds). For providers that call remote systems, account for
network latency.

### `env`

Environment variables in `env` are merged into the child process's environment on
top of the variables inherited from the dotenvz process. If a key exists in both
the inherited environment and `env`, the value in `env` takes precedence.

**Security note:** Values in `env` are stored in `.dotenvz.toml`, which may be
committed to source control. Do not store production secrets or tokens directly
in the `env` table. Instead, use a mechanism such as environment variable
expansion at config-read time, or pass credentials through the OS keychain and
have the provider binary read them.

### `working_dir`

Sets the working directory of the child process. Useful when the provider binary
uses relative paths to find its config files or storage. If omitted, the working
directory is the directory containing `.dotenvz.toml`.

---

---

## Extended Config Model

The following example shows a project that uses AWS in development, GCP in
production, and Azure in a staging environment, all declared in a single
`.dotenvz.toml`:

```toml
# ── Project identity ────────────────────────────────────────────────────────
project         = "my-app"
default_profile = "dev"

# ── Cloud provider declarations ──────────────────────────────────────────────
[providers.aws]
type   = "aws-secrets-manager"
region = "us-east-1"
prefix = "my-app/dev"

[providers.aws-prod]
type   = "aws-secrets-manager"
region = "us-east-1"
prefix = "my-app/prod"

[providers.gcp]
type       = "gcp-secret-manager"
project_id = "my-gcp-project"
prefix     = "my-app-prod"

[providers.azure]
type      = "azure-key-vault"
vault_url = "https://my-vault.vault.azure.net/"
prefix    = "my-app-staging"

# ── Profile bindings ─────────────────────────────────────────────────────────
[profiles.dev]
provider = "aws"

[profiles.staging]
provider = "azure"

[profiles.prod]
provider = "gcp"

# ── Command aliases ───────────────────────────────────────────────────────────
[commands]
dev   = "node server.js"
start = "node server.js"
```

---

## Config Fields Reference

### `[providers.<name>]`

Each entry under `[providers]` declares a named provider instance.
The key (e.g. `aws`, `gcp-prod`) is a local alias used to reference this
provider from profiles.

#### Common fields (all cloud providers)

| Field    | Type   | Required | Description                                                  |
|----------|--------|----------|--------------------------------------------------------------|
| `type`   | string | yes      | Provider type identifier. See table below.                   |
| `prefix` | string | no       | Secret name prefix. Applied before the key name.             |

#### Type values

| `type` value              | Backend                                                       |
|---------------------------|---------------------------------------------------------------|
| `"exec"`                  | External executable (JSON stdin/stdout protocol)              |
| `"aws-secrets-manager"`   | AWS Secrets Manager                                           |
| `"gcp-secret-manager"`    | Google Cloud Secret Manager                                   |
| `"azure-key-vault"`       | Azure Key Vault                                               |

#### Exec-specific fields

See the [Exec Provider](#exec-provider) section at the top of this document for
the full field reference. Exec provider fields (`command`, `args`, `timeout_ms`,
`env`, `working_dir`) are only valid when `type = "exec"`. They are ignored (and
will produce a validation warning in future versions) when combined with cloud
provider types.

#### AWS-specific fields

| Field    | Type   | Required | Description                        |
|----------|--------|----------|------------------------------------|
| `region` | string | yes      | AWS region (e.g. `"us-east-1"`).   |

#### GCP-specific fields

| Field        | Type   | Required | Description                            |
|--------------|--------|----------|----------------------------------------|
| `project_id` | string | yes      | GCP project ID (string or numeric).    |

#### Azure-specific fields

| Field       | Type   | Required | Description                                                  |
|-------------|--------|----------|--------------------------------------------------------------|
| `vault_url` | string | yes      | Full HTTPS URL of the Key Vault.                             |

---

### `[profiles.<name>]`

Each `[profiles.*]` entry binds a profile name (e.g. `dev`, `prod`) to a provider
and can override other top-level config defaults.

| Field      | Type   | Required | Description                                                   |
|------------|--------|----------|---------------------------------------------------------------|
| `provider` | string | no       | Provider alias from `[providers.*]` or a built-in name.      |

When `provider` is set in a profile, it takes precedence over the top-level `provider`
field for that profile. Multiple profiles can reference the same provider declaration.

---

### Top-level `provider` field

The top-level `provider` field (outside any profile) serves as the fallback when no
`[profiles.<active-profile>].provider` is set. It can be either:

- A built-in provider name (`"macos-keychain"`, `"linux-secret-service"`, `"windows-credential"`)
- A cloud provider alias declared in `[providers.*]`

```toml
# Use AWS globally; no per-profile override needed
provider = "aws"

[providers.aws]
type   = "aws-secrets-manager"
region = "eu-west-1"
prefix = "my-app"
```

---

## Providers vs Profiles

| Concept        | Purpose                                                                            |
|----------------|------------------------------------------------------------------------------------|
| `[providers.*]` | Declares a named backend: which cloud, which region/project/vault, which prefix.  |
| `[profiles.*]`  | Binds an environment name (`dev`, `prod`) to a specific provider and can override other settings. |

A provider is re-usable across profiles. A profile is not tied to a single provider.
This allows patterns like:

- Two profiles (`eu-dev`, `us-dev`) sharing the same provider config.
- A single profile (`prod`) switching between AWS and GCP depending on the deployment region.

---

## Environment Separation with `prefix`

The `prefix` field is the primary mechanism for isolating secrets between
environments when using the same cloud account or project.

| Profile   | Provider key | `prefix`           | Resolved secret name example         |
|-----------|--------------|--------------------|---------------------------------------|
| `dev`     | `aws`        | `"my-app/dev"`     | `my-app/dev/DATABASE_URL`             |
| `staging` | `aws`        | `"my-app/staging"` | `my-app/staging/DATABASE_URL`         |
| `prod`    | `aws`        | `"my-app/prod"`    | `my-app/prod/DATABASE_URL`            |

Using a per-environment prefix within the same cloud account is acceptable for
small teams. For strict isolation, use separate cloud accounts, projects, or
subscriptions per environment, and declare a separate `[providers.*]` block for each.

---

## Validation Rules

The following rules are enforced by `DotenvzConfig::validate()`:

1. `type` must be a recognised value; unknown types return a `ConfigParse` error.
2. `command` is required when `type = "exec"`.
3. `command` must be a non-empty string; a path that does not exist on the
   filesystem is a runtime error (not a config validation error).
4. `timeout_ms` must be a non-negative integer when present.
5. `region` is required when `type = "aws-secrets-manager"`.
6. `project_id` is required when `type = "gcp-secret-manager"`.
7. `vault_url` is required when `type = "azure-key-vault"` and must start with `https://`.
8. A `[profiles.*].provider` value must reference either a built-in provider name
   or a key in `[providers.*]`; unknown references return a `ConfigParse` error.
9. `prefix` values must not contain characters that are invalid in the corresponding
   cloud provider's naming scheme (validated per provider).

---

## Migration Guide

If you are currently using a local provider (`macos-keychain`) and want to switch
to a cloud provider for CI/CD:

1. Add a `[providers.aws]` (or equivalent) block to `.dotenvz.toml`.
2. Add a `[profiles.ci]` block with `provider = "aws"`.
3. Run dotenvz with `--profile ci` in your CI pipeline.
4. Ensure the CI environment has the correct IAM role or OIDC token.
5. Pre-provision secrets in AWS Secrets Manager before the first run.

The local provider continues to work for local development profiles.
No secrets are migrated automatically; dotenvz is not a secret synchronisation tool.
