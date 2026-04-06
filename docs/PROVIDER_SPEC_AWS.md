# Provider Spec — AWS Secrets Manager

## Overview

`aws-secrets-manager` is a read-only dotenvz provider that retrieves secrets from
[AWS Secrets Manager](https://docs.aws.amazon.com/secretsmanager/latest/userguide/intro.html)
and injects them as environment variables into child processes.

---

## Configuration

Declare the provider under `[providers.<name>]` in `.dotenvz.toml`.
The provider name is a local alias; the `type` field identifies the backend.

```toml
[providers.aws]
type    = "aws-secrets-manager"
region  = "us-east-1"
prefix  = "my-app/dev"
```

### Config fields

| Field     | Type   | Required | Description                                                                         |
|-----------|--------|----------|-------------------------------------------------------------------------------------|
| `type`    | string | yes      | Must be `"aws-secrets-manager"`.                                                    |
| `region`  | string | yes      | AWS region where the secrets are stored (e.g. `"us-east-1"`).                      |
| `prefix`  | string | no       | Path prefix prepended to every secret name when resolving keys (e.g. `"my-app/dev"`). When omitted, the secret name is used as-is. |

### Activating the provider for a profile

```toml
[profiles.dev]
provider = "aws"

[profiles.prod]
provider = "aws-prod"

[providers.aws-prod]
type   = "aws-secrets-manager"
region = "us-west-2"
prefix = "my-app/prod"
```

---

## Secret Naming and Key Mapping

### One secret per environment variable (default strategy)

Each environment variable corresponds to exactly one secret in AWS Secrets Manager.
The secret name is constructed as:

```
<prefix>/<KEY>
```

If `prefix = "my-app/dev"` and the requested key is `DATABASE_URL`, dotenvz calls
`GetSecretValue` for the secret named `my-app/dev/DATABASE_URL`.

The resulting environment variable is the original key (`DATABASE_URL`), not the full
secret path.

**Naming conventions:**
- Secret names are case-sensitive in AWS Secrets Manager.
- Keys follow the convention used in the dotenvz config (uppercase + underscores).
- The prefix should represent the application and environment (e.g. `myapp/production`).

### JSON blob secret (future)

A future enhancement may support a single AWS secret whose value is a JSON object,
where each key in the object becomes a separate environment variable.

```json
{
  "DATABASE_URL": "postgres://...",
  "API_KEY": "sk-..."
}
```

This is explicitly deferred from the MVP.

---

## Retrieval Flow

1. dotenvz receives a call to `list_secrets(project, profile)`.
2. The provider iterates all keys declared for that project/profile (derived from the
   prefix-enumeration strategy or a manifest; see DOTENVZ_CONFIG_EXTENSIONS.md).
3. For each key, the provider calls the AWS Secrets Manager `GetSecretValue` API:
   - Input: `SecretId = "<prefix>/<KEY>"`
   - The API returns a `SecretString` (UTF-8 text) or `SecretBinary` (base64-encoded bytes).
4. `SecretString` is used directly as the env var value.
5. `SecretBinary` is base64-decoded to UTF-8; if decoding fails, the provider returns
   `DotenvzError::Provider` with a descriptive message.
6. The resulting `HashMap<String, String>` is returned to `env_resolver`.

### Diagram

```
list_secrets("my-app", "dev")
      │
      ├─ prefix = "my-app/dev"
      │
      ├─ for key in [DATABASE_URL, PORT, API_KEY, …]
      │     └─ GetSecretValue(SecretId = "my-app/dev/DATABASE_URL")
      │           └─ SecretString → "postgres://..."
      │
      └─ return { "DATABASE_URL": "postgres://...", "PORT": "5432", … }
```

The AWS SDK handles HTTP connection pooling, retries with exponential back-off,
and credential refresh transparently.

---

## Authentication Sources

dotenvz delegates entirely to the AWS SDK credential provider chain.
No configuration is required in `.dotenvz.toml` for authentication.

| Source                           | Typical environment                         |
|----------------------------------|---------------------------------------------|
| IAM instance profile             | Amazon EC2                                  |
| ECS task role                    | Amazon ECS / AWS Fargate                    |
| Lambda execution role            | AWS Lambda                                  |
| OIDC token file federation       | GitHub Actions, Kubernetes IRSA             |
| `~/.aws/credentials` file        | Developer laptop                            |
| `AWS_ACCESS_KEY_ID` + secret key | Fallback / legacy CI environments           |

**Recommended approach:** Use IAM roles or OIDC federation. Static access keys should
only be used as a last resort and must never be committed to source control or placed
in `.dotenvz.toml`.

### Required IAM permissions

The IAM principal (role, user, or task role) must be granted at minimum:

```json
{
  "Effect": "Allow",
  "Action": [
    "secretsmanager:GetSecretValue"
  ],
  "Resource": "arn:aws:secretsmanager:<region>:<account-id>:secret:<prefix>/*"
}
```

For prefix-enumeration support (future `list_secrets` expansion):

```json
{
  "Effect": "Allow",
  "Action": [
    "secretsmanager:ListSecrets"
  ],
  "Resource": "*"
}
```

---

## Error Handling

| Condition                     | dotenvz error                              | Notes                                                   |
|-------------------------------|---------------------------------------------|---------------------------------------------------------|
| Secret does not exist         | `DotenvzError::KeyNotFound`                | `ResourceNotFoundException` from AWS API                |
| Insufficient IAM permissions  | `DotenvzError::Provider("access denied …")` | `AccessDeniedException` from AWS API                   |
| Secret is pending deletion    | `DotenvzError::Provider("…")`              | `InvalidRequestException` from AWS API                  |
| Network timeout / unreachable | `DotenvzError::Provider("…")`              | Connection or timeout error from the SDK                |
| Region misconfigured          | `DotenvzError::Provider("…")`              | SDK fails to resolve endpoint                           |
| Malformed secret value        | `DotenvzError::Provider("…")`              | Binary secret that cannot be decoded to UTF-8           |

All `DotenvzError::Provider` payloads include the original SDK error message to
aid debugging. Secret values are never included in error messages.

---

## Cost and Rate Limit Awareness

- AWS Secrets Manager charges per API call (GetSecretValue).
- Each secret retrieval is one API call. Applications with many environment variables
  will make proportionally more calls.
- AWS imposes a default quota on `GetSecretValue` (currently 10,000 RPM per region).
- Caching to reduce API calls is deferred to a future release.

---

## Unsupported Operations

The following `SecretProvider` methods are not supported for `aws-secrets-manager`
in the MVP and return `DotenvzError::UnsupportedOperation`:

- `set_secret`
- `delete_secret`

These operations must be performed through the AWS console, AWS CLI, or IaC tooling.
