//! Integration tests for WR-04: check_egress called in run_provider_fallback (D-05).
//! Wave 0 stubs — marked ignored until WR-04 is implemented (Wave 1).

#[test]
#[ignore = "Wave 1 (04-02): run_provider_fallback egress gate not yet wired"]
fn fallback_egress_gate_local_only_blocks_cloud() {
    // Assert: run_provider_fallback with tier=LocalOnly + provider=anthropic returns PrivacyEgressBlocked
    todo!()
}

#[test]
#[ignore = "Wave 1 (04-02): run_provider_fallback egress gate not yet wired"]
fn fallback_egress_gate_local_only_allows_ollama() {
    // Assert: run_provider_fallback with tier=LocalOnly + provider=ollama returns Ok
    todo!()
}

#[test]
#[ignore = "Wave 1 (04-02): run_provider_fallback egress gate not yet wired"]
fn fallback_egress_gate_cloud_ok_allows_all() {
    // Assert: CloudOk tier allows anthropic, openai, gemini, openrouter
    todo!()
}

#[test]
#[ignore = "Wave 1 (04-02): run_provider_fallback egress gate not yet wired"]
fn fallback_egress_gate_none_tier_blocks_all() {
    // Assert: None tier (no MCP tools in turn) is fail-closed — blocks all cloud providers
    todo!()
}
