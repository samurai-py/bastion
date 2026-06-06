//! Integration tests for bastion.toml + env-override config loading (PKG-04).

#[test]
fn config_layering_toml_default_loaded() {
    // Ensure override env var is not set for this test
    std::env::remove_var("BASTION__AGENT__DEFAULT_MODEL");
    let cfg = bastion::config::load_config("bastion.toml")
        .expect("bastion.toml must exist at repo root");
    assert_eq!(cfg.agent.default_model, "claude-sonnet-4-5");
    assert!(cfg.agent.daily_budget_usd > 0.0);
}

#[test]
fn config_layering_env_overrides_toml() {
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
