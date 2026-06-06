//! Integration tests for bastion.toml + env-override config loading (PKG-04).

use std::sync::Mutex;

/// Cargo runs tests in a binary on multiple threads by default, but process environment
/// variables are global shared state. The two tests below both mutate
/// `BASTION__AGENT__DEFAULT_MODEL`, so without serialization one test's set/remove races
/// the other's load → flaky failures. This lock serializes the env-sensitive section.
/// (Poison is recovered: a panic inside the guarded section must not cascade.)
static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn config_layering_toml_default_loaded() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Ensure override env var is not set for this test
    std::env::remove_var("BASTION__AGENT__DEFAULT_MODEL");
    let cfg = bastion::config::load_config("bastion.toml")
        .expect("bastion.toml must exist at repo root");
    assert_eq!(cfg.agent.default_model, "claude-sonnet-4-5");
    assert!(cfg.agent.daily_budget_usd > 0.0);
}

#[test]
fn config_layering_env_overrides_toml() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Set env var override before loading
    std::env::set_var("BASTION__AGENT__DEFAULT_MODEL", "test-model-override");
    let cfg = bastion::config::load_config("bastion.toml")
        .expect("bastion.toml must exist at repo root");
    let model = cfg.agent.default_model.clone();
    // Clean up immediately
    std::env::remove_var("BASTION__AGENT__DEFAULT_MODEL");
    assert_eq!(model, "test-model-override");
}

#[test]
fn config_layering_mcp_servers_folded_in() {
    let cfg = bastion::config::load_config("bastion.toml")
        .expect("bastion.toml must exist at repo root");
    assert!(
        cfg.mcp.servers.contains_key("memupalace"),
        "Expected 'memupalace' in mcp.servers, got: {:?}",
        cfg.mcp.servers.keys().collect::<Vec<_>>()
    );
    assert_eq!(cfg.mcp.servers["memupalace"].url, "http://memupalace:8001/mcp");
}

#[test]
fn config_layering_secrets_not_in_toml() {
    let content = std::fs::read_to_string("bastion.toml")
        .expect("bastion.toml must exist at repo root");
    assert!(
        !content.contains("ANTHROPIC_API_KEY"),
        "bastion.toml must not contain ANTHROPIC_API_KEY"
    );
    assert!(
        !content.contains("TELEGRAM_BOT_TOKEN"),
        "bastion.toml must not contain TELEGRAM_BOT_TOKEN"
    );
    assert!(
        !content.contains("BASTION_INFER_TOKEN"),
        "bastion.toml must not contain BASTION_INFER_TOKEN"
    );
}
