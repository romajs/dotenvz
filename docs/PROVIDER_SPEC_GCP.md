# Provider Spec — Google Cloud Secret Manager

## Overview

`gcp-secret-manager` is a read-only dotenvz provider that retrieves secrets from
[Google Cloud Secret Manager](https://cloud.google.com/secret-manager/docs)
and injects them as environment variables into child processes.

---

## Configuration

```toml
[providers.gcp]
type       = "gcp-secret-manager"
project_id = "my-gcp-project"
prefix     = "my-app-dev"
```

### Config fields

| Field        | Type   | Required | Description                                                                                   |
|--------------|--------|----------|-----------------------------------------------------------------------------------------------|
| `type`       | string | yes      | Must be `"gcp-secret-manager"`.                                                               |
| `project_id` | string | yes      | GCP project ID (numeric or string) where the secrets are stored.                              |
| `prefix`     | string | no       | Name prefix prepended to every secret name (e.g. `"my-app-dev"`). When omitted, the key name is used as-is. |

### Activating the provider for a profile

```toml
[profiles.dev]
provider = "gcp"

[profiles.prod]
provider = "gcp-prod"

[providers.gcp-prod]
type       = "gcp-secret-manager"
project_id = "my-gcp-project"
prefix     = "my-app-prod"
```

---

## Secret Naming and Key Mapping

GCP Secret Manager organizes secrets by **secret name** within a project. Each
secret has one or more numbered versions; dotenvz targets the **`latest`** version
by default.

### Naming strategy

The secret name is constructed as:

```
<prefix>-<KEY>
```

If `prefix = "my-app-dev"` and the key is `DATABASE_URL`, dotenvz accesses the
secret `my-app-dev-DATABASE_URL` in the configured project.

The resource path sent to the GCP API is:

```
projects/<project_id>/secrets/<prefix>-<KEY>/versions/latest
```

**Naming conventions:**
- GCP secret names may contain letters, digits, underscores, and hyphens; they must
  start with a letter or underscore.
- A separator of `-` between prefix and key is the default; if the key itself
  contains hyphens, that is preserved as-is.
- Keys are uppercased by convention (matching the standard env var convention).

### Version pinning (future)

A per-key `version` override may be introduced in a future release to pin secrets
to a specific version number rather than `latest`. This is deferred from the MVP.

---

## Retrieval Flow

1. dotenvz receives a call to `list_secrets(project, profile)`.
2. The provider iterates all keys for the project/profile.
3. For each key, the provider calls the GCP Secret Manager
   `AccessSecretVersion` RPC:
   - Resource: `projects/<project_id>/secrets/<prefix>-<KEY>/versions/latest`
4. The API returns a `SecretPayload` containing a `data` field (bytes).
5. The payload bytes are decoded as UTF-8. If decoding fails, the provider returns
   `DotenvzError::Provider` with a descriptive message.
6. The UTF-8 string is stored in the env map under the original key name.
7. The completed `HashMap<String, String>` is returned to `env_resolver`.

### Diagram

```
list_secrets("my-app", "dev")
      │
      ├─ project_id = "my-gcp-project", prefix = "my-app-dev"
      │
      ├─ for key in [DATABASE_URL, PORT, …]
      │     └─ AccessSecretVersion(
      │             "projects/my-gcp-project/secrets/my-app-dev-DATABASE_URL/versions/latest"
      │          )
      │           └─ payload.data (bytes) → UTF-8 → "postgres://..."
      │
      └─ return { "DATABASE_URL": "postgres://...", "PORT": "5432", … }
```

---

## Authentication Sources

dotenvz delegates to the GCP SDK's **Application Default Credentials (ADC)** chain.
No credential configuration is required in `.dotenvz.toml`.

| Source                                      | Typical environment                              |
|---------------------------------------------|--------------------------------------------------|
| Attached service account (default SA)       | GCE, GKE, Cloud Run, Cloud Functions             |
| Workload Identity Federation                | GitHub Actions, AWS workloads, Azure workloads   |
| `GOOGLE_APPLICATION_CREDENTIALS` env var    | Service account key file (local / CI fallback)   |
| gcloud CLI user credentials                 | Developer laptop (`gcloud auth application-default login`) |

**Recommended approach (production):** Attach a service account with the minimum
required IAM role to the compute resource. Workload Identity Federation is preferred
for CI/CD environments (GitHub Actions, etc.) to avoid long-lived service account keys.

**Discouraged but supported:** `GOOGLE_APPLICATION_CREDENTIALS` pointing to a
service account JSON key file. Key files are long-lived and must be rotated regularly.
Never commit key files to source control.

### Required IAM permissions

The service account or workload identity must be granted at minimum:

- Role: `roles/secretmanager.secretAccessor`
- Scope: specific secrets or the project (depending on blast radius requirements)

For prefix-enumeration (future `ListSecrets` expansion):

- Role: `roles/secretmanager.viewer` (or the custom `secretmanager.secrets.list` permission)

---

## Error Handling

| Condition                        | dotenvz error                               | Notes                                                |
|----------------------------------|----------------------------------------------|------------------------------------------------------|
| Secret does not exist            | `DotenvzError::KeyNotFound`                 | `NOT_FOUND` gRPC status                              |
| No enabled versions              | `DotenvzError::Provider("…")`               | `FAILED_PRECONDITION` — all versions disabled        |
| Insufficient IAM permissions     | `DotenvzError::Provider("access denied …")` | `PERMISSION_DENIED` gRPC status                      |
| ADC not configured               | `DotenvzError::Provider("…")`               | SDK credential resolution failure                    |
| Network timeout / unreachable    | `DotenvzError::Provider("…")`               | gRPC transport error                                 |
| Malformed payload (binary data)  | `DotenvzError::Provider("…")`               | Bytes cannot be decoded as UTF-8                     |

All `DotenvzError::Provider` payloads include the original SDK error message.
Secret values are never included in error messages.

---

## Cost and Rate Limit Awareness

- GCP Secret Manager charges per access operation and per secret version storage.
- Each call to `AccessSecretVersion` is one billed operation.
- GCP imposes per-project quota on `AccessSecretVersion` calls.
- Caching to reduce API calls is deferred to a future release.

---

## Unsupported Operations (MVP)

The following `SecretProvider` methods are not supported for `gcp-secret-manager`
in the MVP and return `DotenvzError::UnsupportedOperation`:

- `set_secret`
- `delete_secret`

Secrets must be managed via the GCP console, `gcloud` CLI, or IaC tooling (Terraform).
