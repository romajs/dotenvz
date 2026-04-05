// Integration tests for dotenvz.
//
// TODO: Add `src/lib.rs` to expose crate internals for integration testing.
//       Until then, these tests exercise public behaviours only.

/// Verify the test fixture config file is a valid TOML document.
/// This test acts as a smoke-check for any hand-edited fixture files.
#[test]
fn fixture_config_is_valid_toml() {
    let raw = include_str!("fixtures/.dotenvz.toml");
    let parsed: toml::Value = toml::from_str(raw).expect("fixture .dotenvz.toml should be valid TOML");
    assert_eq!(parsed["project"].as_str(), Some("test-app"));
    assert_eq!(parsed["provider"].as_str(), Some("macos-keychain"));
    assert!(parsed["commands"]["dev"].as_str().is_some());
}

/// Verify the test fixture .env file contains the expected keys.
#[test]
fn fixture_env_is_readable() {
    let raw = include_str!("fixtures/.env");
    assert!(raw.contains("DATABASE_URL"));
    assert!(raw.contains("API_KEY"));
}
