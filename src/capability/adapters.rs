use crate::capability::registry::{Capability, InvokeCtx};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

/// Wraps an MCP tool dispatch via McpClient.
///
/// McpClient is held as Arc so it can be shared across multiple adapters.
/// Dispatch delegates to call_tool_with_timeout — no business logic in adapter.
pub struct McpToolAdapter {
    pub tool_name: String,
    pub server_label: String,
    pub description: String,
    pub schema: Value,
    /// Shared McpClient reference — injected at registry build time.
    pub mcp: Arc<crate::mcp::McpClient>,
    /// Plan 10-08: typed locality flag, sourced from the owning MCP server's
    /// `[mcp.servers.*].is_local` config (via `ToolRegistry::is_local`) — NEVER
    /// derived from `tool_name`. Mirrors `NlCommandAdapter`'s `is_local()` override
    /// pattern; drives `CapabilityRegistry::invoke`'s egress-provider decision
    /// (`is_local()==true` maps to `"ollama"`, which passes `check_egress` under
    /// `PrivacyTier::LocalOnly`).
    pub is_local_override: bool,
}

#[async_trait]
impl Capability for McpToolAdapter {
    fn name(&self) -> &str {
        &self.tool_name
    }
    fn description(&self) -> &str {
        &self.description
    }
    fn input_schema(&self) -> &Value {
        &self.schema
    }
    fn is_local(&self) -> bool {
        self.is_local_override
    }
    async fn invoke(&self, args: Value, _ctx: &InvokeCtx) -> anyhow::Result<Value> {
        // Delegate to McpClient — no business logic here (thin adapter).
        // call_tool_with_timeout looks up server_label via internal ToolRegistry.
        self.mcp.call_tool_with_timeout(&self.tool_name, args).await
    }
}

/// Wraps a Rust function / SKILL.md built-in.
///
/// Used for DirectFn registrations (SkillsLoader stub at this wave; filled by 04-05).
pub struct DirectFnAdapter {
    pub cap_name: String,
    pub cap_description: String,
    pub schema: Value,
    pub func: Arc<dyn Fn(Value) -> anyhow::Result<Value> + Send + Sync>,
}

#[async_trait]
impl Capability for DirectFnAdapter {
    fn name(&self) -> &str {
        &self.cap_name
    }
    fn description(&self) -> &str {
        &self.cap_description
    }
    fn input_schema(&self) -> &Value {
        &self.schema
    }
    async fn invoke(&self, args: Value, _ctx: &InvokeCtx) -> anyhow::Result<Value> {
        (self.func)(args)
    }
}

/// Wraps a command router entry (slash commands: /stop, /model, /as, etc.)
///
/// NAMING CONTRACT: NlCommandAdapter is registered under the key "cmd:{command_name}"
/// (e.g. "cmd:model", "cmd:stop"). The "cmd:" prefix is a *registry-key convention* only.
///
/// SECURITY: the egress short-circuit is keyed on `is_local()` (a typed property of this
/// adapter), NOT on the name string. The `cmd:` namespace is reserved — `register()` rejects
/// any non-local capability that tries to claim it, so an MCP tool cannot impersonate a
/// local command to bypass egress (D-13 guardrail 3).
///
/// Store `command_name` as "cmd:model" (with prefix), NOT as bare "model".
/// Use `NlCommandAdapter::registry_key(bare)` to build the prefixed form.
pub struct NlCommandAdapter {
    /// Full command name with prefix (e.g. "cmd:model", "cmd:stop", "cmd:as").
    /// MUST start with "cmd:" — this is the egress short-circuit invariant.
    pub command_name: String,
    pub cap_description: String,
    pub schema: Value,
}

impl NlCommandAdapter {
    /// Construct adapter with bare name (e.g. "model") — prefix added automatically.
    pub fn new(
        bare_name: impl Into<String>,
        description: impl Into<String>,
        schema: Value,
    ) -> Self {
        let bare: String = bare_name.into();
        Self {
            command_name: Self::registry_key(&bare),
            cap_description: description.into(),
            schema,
        }
    }

    /// The prefixed registry key for a bare command name: "cmd:model", "cmd:stop", etc.
    pub fn registry_key(bare_name: &str) -> String {
        format!("cmd:{}", bare_name)
    }
}

#[async_trait]
impl Capability for NlCommandAdapter {
    /// Returns "cmd:{command_name}" — the reserved registry-key convention.
    fn name(&self) -> &str {
        &self.command_name
    }
    fn description(&self) -> &str {
        &self.cap_description
    }
    fn input_schema(&self) -> &Value {
        &self.schema
    }
    /// NL commands execute locally via handle_command — they never send data to a cloud
    /// provider, so they are the only adapter type that opts into the local egress path.
    fn is_local(&self) -> bool {
        true
    }
    async fn invoke(&self, _args: Value, _ctx: &InvokeCtx) -> anyhow::Result<Value> {
        // NL commands are dispatched via existing handle_command in src/agent/command.rs.
        // This adapter is a thin shim for registry routing — actual execution happens in AgentLoop.
        // Return a signal value; AgentLoop detects "cmd:" prefix and dispatches to handle_command.
        Ok(serde_json::json!({"cmd": self.command_name, "routed": true}))
    }
}

#[cfg(test)]
mod mcp_tool_adapter_tests {
    use super::*;
    use crate::capability::registry::CapabilityRegistry;
    use crate::mcp::McpClient;
    use crate::memory::PrivacyTier;

    /// No real servers configured — constructs instantly, no network I/O.
    async fn empty_mcp_client() -> Arc<McpClient> {
        Arc::new(
            McpClient::connect_from_config(&std::collections::HashMap::new())
                .await
                .expect("empty server map must construct a client"),
        )
    }

    fn adapter(mcp: Arc<McpClient>, is_local_override: bool) -> McpToolAdapter {
        McpToolAdapter {
            tool_name: "voice_transcribe".to_string(),
            server_label: "voice".to_string(),
            description: "transcribe audio".to_string(),
            schema: serde_json::json!({}),
            mcp,
            is_local_override,
        }
    }

    /// Plan 10-08 Test 1: is_local_override: true => is_local() == true.
    #[tokio::test]
    async fn is_local_override_true_reports_true() {
        let mcp = empty_mcp_client().await;
        let cap = adapter(mcp, true);
        assert!(cap.is_local());
    }

    /// Plan 10-08 Test 2: is_local_override: false => is_local() == false
    /// (existing default-false behavior preserved).
    #[tokio::test]
    async fn is_local_override_false_reports_false() {
        let mcp = empty_mcp_client().await;
        let cap = adapter(mcp, false);
        assert!(!cap.is_local());
    }

    /// Plan 10-08 Test 3 (registry-level regression — the exact guarantee this plan
    /// exists for): an McpToolAdapter with is_local_override: true, registered into a
    /// CapabilityRegistry and invoked through `CapabilityRegistry::invoke` under
    /// `PrivacyTier::LocalOnly`, must NOT be rejected at the egress-check step —
    /// `invoke()` internally derives `provider_for_policy = if cap.is_local() {
    /// "ollama" } else { "external" }` and calls
    /// `check_egress(Some(LocalOnly), provider_for_policy)`, so this asserts the
    /// real production code path, not a parallel direct call to check_egress.
    #[tokio::test]
    async fn is_local_override_true_passes_local_only_egress_check_via_invoke() {
        let mcp = empty_mcp_client().await;
        let cap = adapter(mcp, true);
        let mut registry = CapabilityRegistry::new();
        registry
            .register(Arc::new(cap))
            .expect("local capability must register");

        let ctx = crate::capability::registry::InvokeCtx {
            owner: "test-owner".to_string(),
            privacy_tier: Some(PrivacyTier::LocalOnly),
            needs_approval: false,
        };
        let result = registry
            .invoke("voice_transcribe", serde_json::json!({}), &ctx)
            .await;

        // The actual MCP dispatch will fail (no server is connected in this test),
        // but that failure must come from the McpClient dispatch step, NOT from the
        // egress check — asserting the egress gate itself passed for this adapter.
        if let Err(e) = &result {
            let msg = e.to_string();
            assert!(
                !msg.contains("Privacy egress blocked"),
                "is_local_override:true adapter must pass the egress gate under LocalOnly, got: {msg}"
            );
        }
    }

    /// Companion negative case: is_local_override: false under LocalOnly IS blocked
    /// at the egress step (provider_for_policy resolves to "external").
    #[tokio::test]
    async fn is_local_override_false_is_blocked_by_local_only_egress_via_invoke() {
        let mcp = empty_mcp_client().await;
        let cap = adapter(mcp, false);
        let mut registry = CapabilityRegistry::new();
        registry
            .register(Arc::new(cap))
            .expect("non-local capability must register");

        let ctx = crate::capability::registry::InvokeCtx {
            owner: "test-owner".to_string(),
            privacy_tier: Some(PrivacyTier::LocalOnly),
            needs_approval: false,
        };
        let result = registry
            .invoke("voice_transcribe", serde_json::json!({}), &ctx)
            .await;

        let err = result.expect_err("is_local_override:false must be blocked under LocalOnly");
        assert!(
            err.to_string().contains("Privacy egress blocked"),
            "expected PrivacyEgressBlocked, got: {err}"
        );
    }
}
