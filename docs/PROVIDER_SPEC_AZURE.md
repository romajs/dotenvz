# Provider Spec — Azure Key Vault

## Overview

`azure-key-vault` is a read-only dotenvz provider that retrieves secrets from
[Azure Key Vault](https://learn.microsoft.com/en-us/azure/key-vault/secrets/about-secrets)
and injects them as environment variables into child processes.

---

## Configuration

```toml
[providers.azure]
type      = "azure-key-vault"
vault_url = "https://my-vault.vault.azure.net/"
prefix    = "my-app-dev"
```

### Config fields

| Field       | Type   | Required | Description                                                                                    |
|-------------|--------|----------|------------------------------------------------------------------------------------------------|
| `type`      | string | yes      | Must be `"azure-key-vault"`.                                                                   |
| `vault_url` | string | yes      | The full HTTPS URL of the Key Vault (e.g. `"https://my-vault.vault.azure.net/"`).              |
| `prefix`    | string | no       | Name prefix prepended to every secret name (e.g. `"my-app-dev"`). When omitted, the key name is used as-is. |

### Activating the provider for a profile

```toml
[profiles.staging]
provider = "azure-staging"

[profiles.prod]
provider = "azure-prod"

[providers.azure-staging]
type      = "azure-key-vault"
vault_url = "https://my-staging-vault.vault.azure.net/"
prefix    = "myapp-staging"

[providers.azure-prod]
type      = "azure-key-vault"
vault_url = "https://my-prod-vault.vault.azure.net/"
prefix    = "myapp-prod"
```

---

## Secret Naming and Key Mapping

Azure Key Vault secret names must match the pattern `^[0-9a-zA-Z-]+$` (letters,
digits, and hyphens only). Underscores are **not permitted** in Azure Key Vault
secret names.

### Naming strategy

dotenvz maps environment variable keys to Key Vault secret names using the
following transformation:

1. Underscores (`_`) in the key are replaced with hyphens (`-`).
2. The prefix (if set) is prepended with a `-` separator.
3. The resulting name is lowercased (Key Vault names are case-insensitive but
   lowercasing is the recommended convention).

Example: `prefix = "my-app-dev"`, key = `DATABASE_URL`
→ Key Vault secret name: `my-app-dev-database-url`

The environment variable injected into the process retains the original key name
(`DATABASE_URL`), not the vault secret name.

**Note:** The name transformation is deterministic and reversible. The same mapping
logic is used for both `get_secret` and `list_secrets`.

### Version pinning (future)

Key Vault supports pinning to a specific secret version by appending the version ID.
By default, dotenvz retrieves the current (latest enabled) version. Version pinning
is deferred from the MVP.

---

## Retrieval Flow

1. dotenvz receives a call to `list_secrets(project, profile)`.
2. The provider iterates all keys declared for the project/profile.
3. For each key, the provider calls the Azure Key Vault Secrets API:
   - `GET https://<vault_url>/secrets/<transformed-name>?api-version=7.4`
   - Omitting the version segment returns the latest enabled version.
4. The API returns a JSON response containing a `value` field (string).
5. The `value` string is stored in the env map under the original key name.
6. The completed `HashMap<String, String>` is returned to `env_resolver`.

### Diagram

```
list_secrets("my-app", "dev")
      │
      ├─ vault_url = "https://my-vault.vault.azure.net/", prefix = "my-app-dev"
      │
      ├─ for key in [DATABASE_URL, PORT, …]
      │     └─ key mapping: DATABASE_URL → "my-app-dev-database-url"
      │     └─ GET https://my-vault.vault.azure.net/secrets/my-app-dev-database-url
      │           └─ { "value": "postgres://...", … }
      │
      └─ return { "DATABASE_URL": "postgres://...", "PORT": "5432", … }
```

---

## Authentication Sources

dotenvz delegates to the Azure SDK's **`DefaultAzureCredential`** chain.
No credential configuration is required in `.dotenvz.toml`.

| Source                              | Typical environment                                        |
|-------------------------------------|------------------------------------------------------------|
| System-assigned Managed Identity    | Azure VM, AKS pod, App Service, Functions (no config)     |
| User-assigned Managed Identity      | Same resources; `AZURE_CLIENT_ID` must be set              |
| Workload Identity (AZWI)            | AKS with Azure Workload Identity enabled                   |
| Environment credentials             | `AZURE_CLIENT_ID` + `AZURE_CLIENT_SECRET` + `AZURE_TENANT_ID` |
| Azure CLI (`az login`)              | Developer laptop                                           |
| Azure Developer CLI (`azd`)         | Developer laptop (alternative to `az login`)               |

**Recommended approach (production):** Use system-assigned or user-assigned Managed
Identity on Azure compute resources. This requires zero credential management.

**Supported but discouraged:** Service principal with client secret
(`AZURE_CLIENT_ID` + `AZURE_CLIENT_SECRET`) — secrets must be rotated and stored
securely. Never place `AZURE_CLIENT_SECRET` in `.dotenvz.toml`.

### Required Azure RBAC role

The identity accessing Key Vault must have at minimum:

- Role: **Key Vault Secrets User** (`4633458b-17de-408a-b874-0445c86b69e6`)
- Scope: the specific Key Vault or the resource group containing it.

This role grants `secrets/get` and `secrets/list` permissions, which are both
required for `get_secret` and `list_secrets`.

> **Azure RBAC vs. Access Policies:** If the vault uses the legacy Access Policy
> model rather than Azure RBAC, the identity must be granted `Get` and `List`
> permissions under the vault's access policies. Azure RBAC is preferred for
> new deployments.

---

## Error Handling

| Condition                        | dotenvz error                               | Notes                                                    |
|----------------------------------|----------------------------------------------|----------------------------------------------------------|
| Secret does not exist            | `DotenvzError::KeyNotFound`                 | HTTP 404 from Key Vault                                  |
| Secret is disabled               | `DotenvzError::Provider("…")`               | HTTP 403; secret exists but is not enabled               |
| Insufficient RBAC permissions    | `DotenvzError::Provider("access denied …")` | HTTP 403                                                 |
| Vault not found / URL wrong      | `DotenvzError::Provider("…")`               | HTTP 404 or DNS resolution failure                       |
| Authentication failure           | `DotenvzError::Provider("…")`               | `DefaultAzureCredential` exhausted all sources           |
| Network timeout / unreachable    | `DotenvzError::Provider("…")`               | HTTP connection or timeout error from the SDK            |

All `DotenvzError::Provider` payloads include the original SDK error message.
Secret values are never included in error messages.

---

## Key Name Constraints Reference

| Azure Key Vault constraint | How dotenvz handles it                                    |
|----------------------------|-----------------------------------------------------------|
| No underscores             | Underscores in the env key are replaced with hyphens      |
| Max 127 characters         | Validation error at config load time if exceeded          |
| Case-insensitive           | Names are lowercased by convention                        |
| Alphanumeric + hyphens only | Any other character in the key causes a config error      |

---

## Cost and Rate Limit Awareness

- Azure Key Vault charges per operation (standard tier: ~$0.03 per 10,000 transactions).
- Each `GetSecret` call is one transaction.
- Azure Key Vault imposes a per-vault throttle (approximately 2,000 GET operations
  per 10 seconds on standard vaults).
- Caching to reduce API calls is deferred to a future release.

---

## Unsupported Operations (MVP)

The following `SecretProvider` methods are not supported for `azure-key-vault`
in the MVP and return `DotenvzError::UnsupportedOperation`:

- `set_secret`
- `delete_secret`

Secrets must be managed via the Azure portal, `az keyvault secret set`, or IaC
tooling (Terraform, Bicep).
