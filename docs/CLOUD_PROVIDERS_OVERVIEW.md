# Cloud Providers — Overview

## Overview

dotenvz supports fetching secrets from cloud provider secret stores and injecting them
into processes at runtime, following the same model used for local providers
(macOS Keychain, Linux Secret Service, Windows Credential Manager).

Cloud providers are useful when:

- Running workloads inside cloud platforms (AWS, GCP, Azure).
- Implementing consistent secrets management across environments (dev, staging, prod).
- Enforcing least-privilege access via IAM rather than distributing static credential files.

### Supported cloud providers

| Provider key              | Backend                         |
|---------------------------|----------------------------------|
| `aws-secrets-manager`     | AWS Secrets Manager              |
| `gcp-secret-manager`      | Google Cloud Secret Manager      |
| `azure-key-vault`         | Azure Key Vault                  |

---

## Local vs Cloud Providers

| Dimension              | Local providers                                 | Cloud providers                                         |
|------------------------|--------------------------------------------------|---------------------------------------------------------|
| Storage location       | OS-native keystore (Keychain, Secret Service, …) | Cloud-hosted secret store                               |
| Authentication         | OS user session                                  | IAM role, workload identity, or managed identity        |
| Write support          | Full (set, get, list, delete)                    | Read-only in MVP; write via cloud console or IaC        |
| Latency                | Microseconds (local syscall)                     | Milliseconds (network round-trip)                       |
| Availability           | Always available                                 | Requires network + cloud service availability           |
| Ideal environment      | Local developer machine                          | CI/CD pipelines, containers, serverless functions       |
| Credential storage     | dotenvz manages locally                          | dotenvz does **not** store or manage credentials        |

---

## Why dotenvz Does Not Store Credentials for Cloud Providers

Cloud access credentials (AWS access keys, GCP service account JSON, Azure client secrets)
are themselves sensitive secrets. Storing them inside dotenvz would introduce a
circular trust problem and violate the principle of least privilege.

Instead, dotenvz relies on **ambient credentials** — authentication context that is already
present in the execution environment before dotenvz runs. Each cloud platform provides
native mechanisms for this:

- **AWS** — IAM instance roles, ECS task roles, Lambda execution roles, OIDC federation
  (e.g. GitHub Actions), or `AWS_*` environment variables as a fallback.
- **GCP** — Application Default Credentials (ADC), Workload Identity Federation,
  or `GOOGLE_APPLICATION_CREDENTIALS` pointing to a service account file.
- **Azure** — Managed Identity (system-assigned or user-assigned), Azure CLI login
  (`az login`), or environment variables for a service principal.

This design means:

1. dotenvz does not implement or expose a login command.
2. dotenvz does not cache or proxy cloud tokens.
3. Secret rotation and IAM management remain the responsibility of the platform/operator.
4. The same `.dotenvz.toml` config works on a developer laptop (CLI login) and in
   production (managed identity) without modification.

---

## Read-Only Nature in MVP

Cloud providers are implemented as **read-only** in the initial release.

dotenvz can:
- Fetch a secret value by key (`get_secret`).
- List all secrets for a project/profile (`list_secrets`).

dotenvz will not (in MVP):
- Create or update secrets in the cloud store (`set_secret`).
- Delete secrets from the cloud store (`delete_secret`).

`set_secret` and `delete_secret` called on a cloud provider return
`DotenvzError::UnsupportedOperation`. Secrets must be provisioned through the cloud
console, Infrastructure as Code (Terraform, Pulumi, CDK), or the cloud provider's own CLI.

Write support may be introduced in a future release as an opt-in capability.

---

## Authentication Model

### dotenvz assumes credentials are already present

dotenvz passes through to the cloud SDK's native credential resolution chain.
It does **not**:

- Store API keys, access tokens, or secrets in `.dotenvz.toml`.
- Prompt for cloud credentials interactively.
- Perform OAuth flows.
- Manage token lifetimes or refresh cycles.

### Per-provider credential resolution order

**AWS Secrets Manager**

The AWS SDK for Rust resolves credentials in this order:

1. `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` / `AWS_SESSION_TOKEN` environment variables
2. AWS shared credentials file (`~/.aws/credentials`)
3. AWS config file (`~/.aws/config`)
4. ECS container credentials (via `AWS_CONTAINER_CREDENTIALS_RELATIVE_URI`)
5. EC2 instance metadata service (IMDSv2)
6. OIDC token file (for GitHub Actions and Kubernetes service account projection)

**Google Cloud Secret Manager**

The GCP SDK resolves credentials via Application Default Credentials (ADC):

1. `GOOGLE_APPLICATION_CREDENTIALS` environment variable (path to service account JSON)
2. gcloud CLI user credentials (`~/.config/gcloud/application_default_credentials.json`)
3. Attached service account (GCE, GKE, Cloud Run, Cloud Functions)
4. Workload Identity Federation (GitHub Actions, AWS, Azure)

**Azure Key Vault**

The Azure SDK resolves credentials via `DefaultAzureCredential`, in order:

1. `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `AZURE_TENANT_ID` environment variables
2. Workload Identity (Kubernetes + Azure AD)
3. Managed Identity (user-assigned when `AZURE_CLIENT_ID` is set, otherwise system-assigned)
4. Azure CLI (`az login`)
5. Azure Developer CLI (`azd auth login`)
6. Azure PowerShell / Visual Studio credential

---

## Execution Flow

The end-to-end flow when a cloud provider is active is as follows:

```
dotenvz exec -- my-service
      │
      1. ProjectContext::resolve()
      │     └─ walk cwd → find .dotenvz.toml → parse → pick active profile
      │
      2. build_provider(&ctx)
      │     └─ read ctx.config.provider (or profile-level override)
      │     └─ match type string:
      │           "aws-secrets-manager" → AwsSecretsManagerProvider::new(&cfg)
      │           "gcp-secret-manager"  → GcpSecretManagerProvider::new(&cfg)
      │           "azure-key-vault"     → AzureKeyVaultProvider::new(&cfg)
      │
      3. Ambient authentication
      │     └─ SDK reads credentials from environment / metadata endpoint
      │     └─ dotenvz has no role in this step
      │
      4. env_resolver::resolve_env(&provider, project, profile)
      │     └─ provider.list_secrets(project, profile)
      │           └─ iterate declared keys (or prefix-enumerate)
      │           └─ call cloud API for each secret
      │           └─ return HashMap<String, String>
      │
      5. Key mapping
      │     └─ cloud secret name → environment variable name (see each spec)
      │
      6. process_runner::run_command_string("my-service", &env)
            └─ spawn child with merged env (process.env + injected secrets)
```

Steps 1–2 are identical to the local provider flow. The cloud-specific work
happens in steps 3–5 inside the provider implementation.

---

## Security Summary

- Credentials flow through the platform's native chain; dotenvz never touches them.
- Secret values are held in memory only long enough to build the env map and spawn
  the child process.
- Secret values are never written to disk, logged, or surfaced in `--dry-run` output
  (dry-run shows key names as `<redacted>`).
- Each cloud provider should be granted only the minimum IAM permissions required
  (typically `GetSecretValue` or `AccessSecretVersion`).

See [Security Considerations](#) in `IMPLEMENTATION_GUIDE.md` for a detailed treatment.
