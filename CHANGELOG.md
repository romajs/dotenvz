# Changelog

All notable changes to this project will be documented in this file.

## [0.1.0] - 2026-04-06

### Features

- Initial implementation: `dotenvz init`, `set`, `get`, `list`, `rm`, `import`, `exec`, and named command aliases
- macOS Passwords / iCloud Keychain provider (`macos-passwords`) — synchronizable items visible in Passwords.app
- macOS local login Keychain provider (`macos-keychain`) — local-only, no iCloud sync
- Linux Secret Service provider via D-Bus (`secret-service` crate, supports GNOME Keyring and KWallet)
- Windows Credential Manager provider via Win32 `Cred*` API (`windows-sys`)
- `dz` binary alias — short-form entry point for all commands
- Per-project configuration via `.dotenvz.toml` with profile support
- Named command aliases with automatic env injection (`dotenvz dev`, `dotenvz build`, etc.)
- Explicit exec mode: `dotenvz exec -- <command> [args...]`
- One-time import from `.env` into the OS secret store (`dotenvz import`)
- GitHub Actions CI pipeline — build, test, and clippy on macOS, Linux, and Windows
- Cross-platform binary release pipeline — native and cross-compiled builds for 7 targets published to GitHub Releases with SHA256 checksums

