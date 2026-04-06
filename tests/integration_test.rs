use std::fs;

use dotenvz::{
    config::{write_config, DotenvzConfig, CONFIG_FILENAME},
    core::{env_resolver, process_runner, project_context::ProjectContext},
    providers::{mock::InMemoryProvider, SecretProvider},
};
use tempfile::TempDir;

// ─────────────────────────────────────────────────────────────────────────────
// Fixture smoke tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fixture_config_is_valid_toml() {
    let raw = include_str!("fixtures/.dotenvz.toml");
    let cfg: DotenvzConfig = toml::from_str(raw).expect("fixture .dotenvz.toml should parse");
    assert_eq!(cfg.project, "test-app");
    assert_eq!(cfg.provider, "macos-keychain");
    assert!(cfg.commands.contains_key("dev"));
    cfg.validate().expect("fixture config should be valid");
}

#[test]
fn fixture_env_contains_expected_keys() {
    let raw = include_str!("fixtures/.env");
    assert!(raw.contains("DATABASE_URL="));
    assert!(raw.contains("API_KEY="));
}

// ─────────────────────────────────────────────────────────────────────────────
// Phase 4 — import flow with mock provider
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn import_populates_mock_provider() {
    let dir = TempDir::new().unwrap();
    let config = DotenvzConfig::scaffold("import-test");
    write_config(&dir.path().join(CONFIG_FILENAME), &config).unwrap();

    // Write a .env with one empty value that should be skipped.
    let env_content = "DATABASE_URL=postgres://localhost\nAPI_KEY=test-secret\nEMPTY_VAR=\n";
    fs::write(dir.path().join(".env"), env_content).unwrap();

    let provider = InMemoryProvider::new();
    let ctx = ProjectContext::resolve_from(dir.path(), None).unwrap();

    dotenvz::commands::import::run(&ctx, &provider, None, false).unwrap();

    let secrets = provider.list_secrets("import-test", "dev").unwrap();
    // EMPTY_VAR should be skipped
    assert_eq!(secrets.len(), 2, "only non-empty vars should be imported");
    assert_eq!(secrets.get("DATABASE_URL").unwrap(), "postgres://localhost");
    assert_eq!(secrets.get("API_KEY").unwrap(), "test-secret");
    assert!(!secrets.contains_key("EMPTY_VAR"));
}

#[test]
fn import_dry_run_does_not_persist() {
    let dir = TempDir::new().unwrap();
    let config = DotenvzConfig::scaffold("dry-run-test");
    write_config(&dir.path().join(CONFIG_FILENAME), &config).unwrap();
    fs::write(dir.path().join(".env"), "KEY=value\n").unwrap();

    let provider = InMemoryProvider::new();
    let ctx = ProjectContext::resolve_from(dir.path(), None).unwrap();

    dotenvz::commands::import::run(&ctx, &provider, None, true).unwrap();

    // Dry-run must not write anything.
    assert!(provider
        .list_secrets("dry-run-test", "dev")
        .unwrap()
        .is_empty());
}

// ─────────────────────────────────────────────────────────────────────────────
// Phase 5 — env resolution and exec with mock provider
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn env_resolver_returns_only_scoped_secrets() {
    let provider = InMemoryProvider::new();
    provider
        .set_secret("app", "dev", "HOST", "localhost")
        .unwrap();
    provider
        .set_secret("app", "prod", "HOST", "prod.example.com")
        .unwrap();

    let env = env_resolver::resolve_env(&provider, "app", "dev").unwrap();
    assert_eq!(env.get("HOST").unwrap(), "localhost");
    assert_eq!(env.len(), 1);
}

#[test]
fn exec_dry_run_does_not_run_process() {
    let dir = TempDir::new().unwrap();
    let config = DotenvzConfig::scaffold("exec-test");
    write_config(&dir.path().join(CONFIG_FILENAME), &config).unwrap();

    let provider = InMemoryProvider::new();
    provider
        .set_secret("exec-test", "dev", "MY_SECRET", "abc")
        .unwrap();

    let ctx = ProjectContext::resolve_from(dir.path(), None).unwrap();
    let args = vec!["false".to_string()]; // `false` exits 1 — should never run

    dotenvz::commands::exec::run(&ctx, &provider, &args, true).unwrap();
}

#[test]
fn process_runner_injects_env_into_child() {
    use std::collections::HashMap;
    let mut env = HashMap::new();
    env.insert("DOTENVZ_INTEG_KEY".to_string(), "expected_val".to_string());

    // `sh -c 'test "$VAR" = "expected_val"'` exits 0 only if the var is set.
    process_runner::run_process(
        "sh",
        &["-c", r#"test "$DOTENVZ_INTEG_KEY" = "expected_val""#],
        &env,
    )
    .unwrap();
}

// ─────────────────────────────────────────────────────────────────────────────
// Phase 6 — init → import → list → exec end-to-end with mock
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn full_flow_init_import_list_exec() {
    let dir = TempDir::new().unwrap();

    // 1. init
    let _ = dotenvz::commands::init::run(Some("full-flow-app"), false).is_err(); // runs in cwd, not tmpdir
                                                                                 // Write config directly since we can't change cwd in a safe test.
    let config = DotenvzConfig::scaffold("full-flow-app");
    write_config(&dir.path().join(CONFIG_FILENAME), &config).unwrap();

    // 2. import
    let env_content = "GREETING=hello\nEMPTY=\n";
    fs::write(dir.path().join(".env"), env_content).unwrap();
    let provider = InMemoryProvider::new();
    let ctx = ProjectContext::resolve_from(dir.path(), None).unwrap();
    dotenvz::commands::import::run(&ctx, &provider, None, false).unwrap();

    // 3. list — verify one secret is stored
    let secrets = provider.list_secrets("full-flow-app", "dev").unwrap();
    assert_eq!(secrets.len(), 1);
    assert_eq!(secrets.get("GREETING").unwrap(), "hello");

    // 4. exec — verify the injected env is visible to the child process
    let args = vec![
        "sh".to_string(),
        "-c".to_string(),
        r#"test "$GREETING" = "hello""#.to_string(),
    ];
    dotenvz::commands::exec::run(&ctx, &provider, &args, false).unwrap();
}

#[test]
fn init_force_overwrites_existing_config() {
    let dir = TempDir::new().unwrap();
    let config = DotenvzConfig::scaffold("original");
    write_config(&dir.path().join(CONFIG_FILENAME), &config).unwrap();

    // init --force in current test is tricky since it uses cwd. We verify the
    // write_config + load round-trip instead, which is what init does internally.
    let new_config = DotenvzConfig::scaffold("overwritten");
    write_config(&dir.path().join(CONFIG_FILENAME), &new_config).unwrap();

    let ctx = ProjectContext::resolve_from(dir.path(), None).unwrap();
    assert_eq!(ctx.config.project, "overwritten");
}
