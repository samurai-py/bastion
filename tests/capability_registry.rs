//! Integration tests for CapabilityRegistry policy middleware (D-13).
//! Wave 0 stubs — marked ignored until registry is implemented (Wave 3).

#[test]
#[ignore = "Wave 3 (04-04): CapabilityRegistry not yet implemented"]
fn capability_registry_policy_local_only_blocked() {
    // Assert: registry.invoke with tier=LocalOnly + non-ollama provider returns PrivacyEgressBlocked
    todo!()
}

#[test]
#[ignore = "Wave 3 (04-04): CapabilityRegistry not yet implemented"]
fn capability_registry_all_frontends_pass_through_policy() {
    // Assert: direct fn / MCP tool / NL command all pass through same policy check
    todo!()
}

#[test]
#[ignore = "Wave 3 (04-04): CapabilityRegistry not yet implemented"]
fn capability_registry_unknown_capability_returns_error() {
    // Assert: invoke("unknown_cap", ...) returns Err (not panic)
    todo!()
}
