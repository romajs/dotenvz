# Custom Provider Protocol — dotenvz Exec Provider

## Overview

### What is the exec provider?

The exec provider allows dotenvz to delegate all secret operations to an external
executable of your choice. Instead of using a built-in backend (macOS Keychain,
AWS Secrets Manager, etc.), dotenvz spawns a process you supply and communicates
with it over a simple JSON protocol on stdin/stdout.

This mechanism is intentionally minimal. It exists as an escape hatch — the right
choice when none of the built-in providers fits your needs, when you need to bridge
an internal secret system, or when you want to prototype a new provider without
modifying dotenvz itself.

### When to use it

- Your secrets live in an internal system not covered by a built-in provider.
- You need to add custom logic before or after secret retrieval (audit logging,
  rate limiting, caching).
- You are developing a new provider and want to iterate quickly without recompiling.
- You need team-specific behaviour that is too niche to upstream.

### When NOT to use it

- A built-in provider already covers your backend. Prefer it — it avoids the extra
  process launch overhead and is fully tested.
- You need persistence across multiple dotenvz invocations. The exec provider is
  stateless by design.
- You want a plugin that loads into the dotenvz process. The exec provider is
  out-of-process by design; no dynamic loading is supported.

### Security model

The exec provider is **local-only**. dotenvz:

- Only spawns executables at absolute or explicitly configured paths on the local
  filesystem.
- Does NOT fetch executables from the network.
- Does NOT use shell interpolation to construct the command.
- Does NOT expose secrets via process environment or command-line arguments.
- Communicates exclusively over the child process's stdin/stdout file descriptors,
  with stderr reserved for diagnostics.

The security boundary is the path and permissions you configure. You are responsible
for ensuring the executable path is trusted, not world-writable, and not a symlink
to an untrusted binary. See the Security Considerations section for a full checklist.

---

## Communication Model

### Process lifecycle

dotenvz uses a **one-process-per-request** model.

For each secret operation (`get`, `set`, `list`, `rm`), dotenvz:

1. Spawns the configured executable.
2. Writes exactly one JSON request object to the child's stdin, terminated by a
   newline (`\n`).
3. Closes the stdin pipe (EOF).
4. Reads the child's stdout until EOF.
5. Parses the response JSON.
6. Waits for the child process to exit.

The process is not kept alive between requests. There is no persistent daemon,
socket, or IPC channel.

**Rationale:** A persistent process would require a daemon lifecycle, health checks,
restart logic, and locking to prevent concurrent calls from different dotenvz
invocations. The one-request model eliminates all of this. The overhead of a process
launch is acceptable for the use cases exec providers are designed for. If performance
is critical, write a built-in provider instead.

### Encoding

All communication uses **UTF-8**. Both the request and response payloads MUST be
valid UTF-8. The JSON request will never contain non-UTF-8 bytes. If your provider
implementation reads bytes, treat them as UTF-8.

### Framing

- The request is a single JSON object followed by exactly one newline (`\n`) byte.
- The response is a single JSON object followed by exactly one newline (`\n`) byte.
- Multiple JSON objects on stdout are not supported. dotenvz reads stdout until EOF
  and parses the entire content as a single JSON value. Extra trailing whitespace is
  tolerated; extra non-whitespace after the first complete JSON object is an error.

### Blocking

The communication is fully blocking (synchronous). dotenvz does not use async I/O
to communicate with the child process. The provider executable is expected to write
its response and exit within the configured `timeout_ms`.

### Stderr

Stderr is **not read** by dotenvz. It is inherited from the dotenvz process, so
diagnostic output from your provider appears in the user's terminal. This is
intentional — it lets you log warnings, deprecation notices, or debug output without
polluting the protocol channel.

**NEVER write secrets or partial JSON to stderr.** Treat it as a terminal-visible
diagnostic channel only.

### Exit codes

| Exit code | Meaning                                                    |
|-----------|------------------------------------------------------------|
| `0`       | Protocol exchange completed. Check the `ok` field in the JSON response for logical success or failure. |
| Non-zero  | The provider process itself failed before or after writing a response. dotenvz treats a non-zero exit code combined with empty or invalid stdout as an `INTERNAL_ERROR`. If valid JSON was received on stdout, the JSON response takes precedence over the exit code for error classification. |

dotenvz always reads stdout before checking the exit code. A non-zero exit after
writing a valid error response is treated as the JSON error, not a crash.

---

## Request Format

All requests are JSON objects. Every request includes:

| Field     | Type   | Required | Description                                        |
|-----------|--------|----------|----------------------------------------------------|
| `action`  | string | yes      | Operation to perform. See table below.             |
| `project` | string | yes      | Project name from `.dotenvz.toml`.                 |
| `profile` | string | yes      | Active profile name (e.g. `"dev"`, `"prod"`).      |
| `key`     | string | no       | Secret key name. Required for `get`, `set`, `rm`.  |
| `value`   | string | no       | Secret value. Required for `set` only.             |

### Supported actions

| `action`  | Description                                               | Required fields beyond `action`, `project`, `profile` |
|-----------|-----------------------------------------------------------|--------------------------------------------------------|
| `get`     | Retrieve a single secret's value.                         | `key`                                                  |
| `set`     | Store or overwrite a single secret.                       | `key`, `value`                                         |
| `list`    | Return all key names for the given project/profile.       | _(none)_                                               |
| `rm`      | Delete a single secret.                                   | `key`                                                  |
| `health`  | Optional liveness check. No secret operations performed.  | _(none)_                                               |

`health` is optional. dotenvz never calls it automatically; it is available for
operators who want to probe the provider before a deployment.

### Examples

**get**

```json
{
  "action": "get",
  "project": "my-app",
  "profile": "dev",
  "key": "DATABASE_URL"
}
```

**set**

```json
{
  "action": "set",
  "project": "my-app",
  "profile": "dev",
  "key": "API_KEY",
  "value": "secret-value-here"
}
```

**list**

```json
{
  "action": "list",
  "project": "my-app",
  "profile": "dev"
}
```

**rm**

```json
{
  "action": "rm",
  "project": "my-app",
  "profile": "dev",
  "key": "OLD_API_KEY"
}
```

**health**

```json
{
  "action": "health",
  "project": "my-app",
  "profile": "dev"
}
```

---

## Response Format

### Success response (get / set / rm / health)

```json
{
  "ok": true,
  "value": "secret-value-here"
}
```

`value` is present for `get`. It is omitted or `null` for `set`, `rm`, and `health`.

### Success response (list)

```json
{
  "ok": true,
  "keys": ["DATABASE_URL", "API_KEY", "REDIS_URL"]
}
```

`keys` is an array of secret key name strings for the given project/profile.
An empty project/profile returns `"keys": []`, which is a valid success response.

### Error response

```json
{
  "ok": false,
  "error": {
    "code": "NOT_FOUND",
    "message": "Secret 'DATABASE_URL' not found in profile 'dev'"
  }
}
```

| Field           | Type   | Required | Description                                           |
|-----------------|--------|----------|-------------------------------------------------------|
| `ok`            | bool   | yes      | Always `false` for errors.                            |
| `error.code`    | string | yes      | Machine-readable error code. See Error Codes section. |
| `error.message` | string | yes      | Human-readable description. May appear in CLI output. |

The `message` field is displayed to the user. Do not include raw secrets, stack
traces, or internal paths in this field.

### Response field summary

| Field   | Present when     | Type            | Description                              |
|---------|------------------|-----------------|------------------------------------------|
| `ok`    | always           | bool            | `true` on success, `false` on error.     |
| `value` | `get` success    | string          | The secret's string value.               |
| `keys`  | `list` success   | array of string | All key names in the project/profile.    |
| `error` | any failure      | object          | Contains `code` and `message`.           |

---

## Error Codes

The following error codes are defined. dotenvz maps each to a `DotenvzError` variant;
unknown codes are treated as `INTERNAL_ERROR`.

| Code              | Meaning                                                                 | Mapped to                     |
|-------------------|-------------------------------------------------------------------------|-------------------------------|
| `NOT_FOUND`       | The requested key does not exist in the provider.                       | `DotenvzError::KeyNotFound`   |
| `ACCESS_DENIED`   | The provider binary lacks permission to access the secret.              | `DotenvzError::Provider`      |
| `INVALID_REQUEST` | The request JSON was malformed, or a required field was missing.        | `DotenvzError::Provider`      |
| `INTERNAL_ERROR`  | The provider encountered an unexpected error.                           | `DotenvzError::Provider`      |
| `TIMEOUT`         | The provider is reporting its own internal timeout (distinct from the dotenvz-level timeout). | `DotenvzError::Provider` |

dotenvz also imposes its own timeout (see Timeout Behavior). If dotenvz kills the
process due to timeout, it raises `DotenvzError::Provider("provider timed out")`
regardless of what the provider may have written.

---

## Security Considerations

### No remote execution

dotenvz does not download, verify, or cache provider executables. The `command`
field in `.dotenvz.toml` must point to a binary already present on the local
filesystem. dotenvz will not execute a URL or a script fetched over the network.

### User-installed binaries only

The provider executable is run with the same Unix user identity as the dotenvz
process. It inherits no elevated privileges. If your provider requires elevated
access (e.g. reading from `/etc/secrets`), that must be handled by the binary
itself via `sudo`, `setuid`, or operating system capabilities — not by dotenvz.

### Path validation recommendations

- Use absolute paths. Relative paths are evaluated against `working_dir` (if set)
  or the directory containing `.dotenvz.toml`. Prefer absolute paths to avoid
  ambiguity.
- Ensure the path is not world-writable (`chmod o-w /path/to/provider`).
- Do not use `/tmp` or other world-writable directories for provider binaries.
- Verify the binary is owned by a trusted user before deployment.

### Avoiding shell injection

dotenvz constructs the child process using `std::process::Command`. The `command`
field is used directly as the executable path. The `args` array elements are passed
as separate arguments — there is no shell involved. Shell metacharacters (`|`, `;`,
`$`, backticks) in `args` values are passed as literal strings to the executable,
not interpreted by a shell.

**Never** set `command` to a shell interpreter (e.g. `"/bin/sh"`) with `args`
containing untrusted user input. If you need a shell script, write a wrapper
script, make it executable, and point `command` at the wrapper.

### Timeouts and process kill

Every exec provider invocation is subject to `timeout_ms`. When the timeout
expires, dotenvz sends `SIGKILL` to the child process on Unix (or calls
`TerminateProcess` on Windows). The process cannot catch or ignore this signal.

Ensure your provider implementation writes responses and exits promptly. Do not
rely on cleanup logic that runs after the main response write, as the process may
be killed before it completes.

### Stdout/stderr separation

dotenvz reads **only stdout** for the JSON response. The provider MUST write the
JSON response exclusively to stdout. Any error diagnostics, debug output, or log
messages must go to stderr.

Writing partial JSON to stdout before an error, then writing more JSON to stdout
after, produces unparseable output and causes an `INTERNAL_ERROR` from dotenvz's
perspective.

### No logging secrets

The `message` field in error responses and any stderr output are visible to users
and may appear in CI logs. Never include secret values, access tokens, or credentials
in these fields. Log the key name if needed, but not the value.

---

## Timeout Behavior

### Configuration

```toml
[providers.custom]
type        = "exec"
command     = "/usr/local/bin/my-provider"
timeout_ms  = 5000
```

`timeout_ms` is in milliseconds. The default is `5000` (5 seconds) if not configured.
A value of `0` disables the timeout entirely (not recommended for production).

### What happens on timeout

1. dotenvz spawns the child process and writes the request to stdin.
2. dotenvz waits for the response with a deadline equal to `timeout_ms` from spawn
   time (not from when stdin was closed).
3. If the deadline is reached before stdout is fully read:
   - dotenvz sends `SIGKILL` (Unix) or calls `TerminateProcess` (Windows).
   - dotenvz returns `DotenvzError::Provider("exec provider timed out after 5000ms")`.
   - Any partial stdout content is discarded.

### Process kill safety

`SIGKILL` cannot be caught or deferred. The child process is terminated immediately.
If your provider needs to flush a write buffer or release a lock on timeout, you
cannot rely on in-process cleanup after `SIGKILL`. Design your provider so that
interrupted executions leave the backing store in a consistent state — for example,
by using atomic writes.

---

## Example Provider Implementation

The following example implements a minimal provider in Python backed by a local
JSON file. It is intentionally simple: a reference for protocol compliance, not a
production implementation.

```python
#!/usr/bin/env python3
"""
dotenvz exec provider — example implementation (Python)

Backed by a JSON file at ~/.config/my-provider/secrets.json with structure:
  {
    "<project>/<profile>/<key>": "<value>",
    ...
  }

Protocol: read one JSON line from stdin, write one JSON line to stdout.
"""

import json
import os
import sys

STORE_PATH = os.path.expanduser("~/.config/my-provider/secrets.json")


def load_store():
    if not os.path.exists(STORE_PATH):
        return {}
    with open(STORE_PATH) as f:
        return json.load(f)


def save_store(store):
    os.makedirs(os.path.dirname(STORE_PATH), exist_ok=True)
    tmp = STORE_PATH + ".tmp"
    with open(tmp, "w") as f:
        json.dump(store, f)
    os.replace(tmp, STORE_PATH)  # atomic rename


def make_key(project, profile, key):
    return f"{project}/{profile}/{key}"


def ok(data=None):
    response = {"ok": True}
    if data:
        response.update(data)
    return response


def err(code, message):
    return {"ok": False, "error": {"code": code, "message": message}}


def handle(request):
    action  = request.get("action")
    project = request.get("project", "")
    profile = request.get("profile", "")
    key     = request.get("key", "")
    value   = request.get("value", "")

    if not action:
        return err("INVALID_REQUEST", "Missing required field: action")

    store = load_store()
    composite = make_key(project, profile, key)

    if action == "health":
        return ok()

    elif action == "get":
        if not key:
            return err("INVALID_REQUEST", "Missing required field: key")
        if composite not in store:
            return err("NOT_FOUND", f"Secret '{key}' not found in profile '{profile}'")
        return ok({"value": store[composite]})

    elif action == "set":
        if not key:
            return err("INVALID_REQUEST", "Missing required field: key")
        if not value:
            return err("INVALID_REQUEST", "Missing required field: value")
        store[composite] = value
        save_store(store)
        return ok()

    elif action == "list":
        prefix = make_key(project, profile, "")
        keys = [
            k.removeprefix(prefix)
            for k in store
            if k.startswith(prefix)
        ]
        return ok({"keys": keys})

    elif action == "rm":
        if not key:
            return err("INVALID_REQUEST", "Missing required field: key")
        if composite not in store:
            return err("NOT_FOUND", f"Secret '{key}' not found in profile '{profile}'")
        del store[composite]
        save_store(store)
        return ok()

    else:
        return err("INVALID_REQUEST", f"Unknown action: '{action}'")


def main():
    raw = sys.stdin.read().strip()
    if not raw:
        print(json.dumps(err("INVALID_REQUEST", "Empty request")))
        sys.exit(1)

    try:
        request = json.loads(raw)
    except json.JSONDecodeError as e:
        print(json.dumps(err("INVALID_REQUEST", f"Malformed JSON: {e}")))
        sys.exit(1)

    response = handle(request)
    print(json.dumps(response))
    sys.exit(0)


if __name__ == "__main__":
    main()
```

### Notes on the example

- The file is made executable: `chmod +x /usr/local/bin/my-provider`.
- Secrets are stored atomically using a rename. An interrupted `set` does not
  corrupt the store.
- stderr is not used in the happy path. Production code may write to `sys.stderr`
  for diagnostics.
- The example does not implement authentication or encryption. A real provider
  would use OS-level key management or a hardware security module for the store key.
- Error codes match exactly the defined set. Unknown codes on the dotenvz side
  fall back to `INTERNAL_ERROR`, so precision here matters for good UX.
