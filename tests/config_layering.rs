//! Integration tests for bastion.toml + env-override config loading (PKG-04).
//! Wave 0 stubs — all tests marked ignored until bastion.toml loader is implemented (Wave 3).

#[test]
#[ignore = "Wave 3: bastion.toml loader not yet implemented"]
fn config_layering_toml_default_loaded() {
    // Assert: load_config("bastion.toml") returns BastionConfig with agent.default_model = "claude-sonnet-4-5"
    todo!()
}

#[test]
#[ignore = "Wave 3: bastion.toml loader not yet implemented"]
fn config_layering_env_overrides_toml() {
    // Assert: BASTION__AGENT__DEFAULT_MODEL=test-model overrides toml value
    todo!()
}

#[test]
#[ignore = "Wave 3: bastion.toml loader not yet implemented"]
fn config_layering_mcp_servers_folded_in() {
    // Assert: config.mcp.servers["memupalace"].url == "http://memupalace:8001/mcp"
    todo!()
}

#[test]
#[ignore = "Wave 3: bastion.toml loader not yet implemented"]
fn config_layering_secrets_not_in_toml() {
    // Assert: bastion.toml content does not contain ANTHROPIC_API_KEY, TELEGRAM_BOT_TOKEN
    todo!()
}
