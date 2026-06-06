use std::collections::HashMap;
use std::sync::Arc;
use serde_json::Value;
use async_trait::async_trait;
use crate::memory::PrivacyTier;

/// Invocation context — resolved BEFORE entering registry.invoke.
pub struct InvokeCtx {
    pub owner: String,
    pub privacy_tier: Option<PrivacyTier>,
    /// If true, the approval queue gate is activated (Phase 3 approval flow).
    pub needs_approval: bool,
}

/// A capability is anything the agent can invoke through the registry.
#[async_trait]
pub trait Capability: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> &Value;
    async fn invoke(&self, args: Value, ctx: &InvokeCtx) -> anyhow::Result<Value>;
}

/// Unified capability registry.
///
/// Single policy enforcement point — every frontend (direct fn, MCP tool, NL command)
/// invokes through here. check_egress is called once per invoke at this boundary.
pub struct CapabilityRegistry {
    inner: HashMap<String, Arc<dyn Capability>>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self { inner: HashMap::new() }
    }

    pub fn register(&mut self, cap: Arc<dyn Capability>) {
        self.inner.insert(cap.name().to_owned(), cap);
    }

    pub fn list_names(&self) -> Vec<&str> {
        self.inner.keys().map(|s| s.as_str()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Single policy enforcement point (D-13 non-negotiable guardrail).
    ///
    /// Policy order:
    /// 1. Egress check — fail-closed on LocalOnly or None tier for non-local adapters
    /// 2. Approval gate — if ctx.needs_approval, gate on Phase 3 queue
    /// 3. Dispatch to capability adapter
    pub async fn invoke(
        &self,
        name: &str,
        args: Value,
        ctx: &InvokeCtx,
    ) -> anyhow::Result<Value> {
        let cap = self.inner.get(name)
            .ok_or_else(|| anyhow::anyhow!("unknown capability: {}", name))?;

        // Policy 1: egress check
        // NlCommandAdapters are local (no egress) — they register under "cmd:" prefix.
        // "cmd:" prefix → use "ollama" as provider_name → always passes check_egress.
        // This is correct: NL commands never send data to cloud providers.
        // MCP tools and DirectFn adapters use "external" — LocalOnly blocks them.
        let provider_for_policy = if cap.name().starts_with("cmd:") {
            "ollama"  // NL commands are local — always pass egress check
        } else {
            // MCP tools and DirectFn: use "external" so LocalOnly tier blocks them.
            // check_egress(Some(LocalOnly), "external") → Err(PrivacyEgressBlocked)
            // check_egress(Some(CloudOk), "external") → Ok(())
            "external"
        };
        crate::hooks::egress::check_egress(ctx.privacy_tier, provider_for_policy)?;

        // Policy 2: approval gate (Phase 3 queue wiring — currently no-op stub)
        if ctx.needs_approval {
            // TODO Phase 3: gate on approval_queue
            tracing::debug!(event = "approval_gate_noop", capability = %name);
        }

        // Dispatch
        cap.invoke(args, ctx).await
    }
}

impl Default for CapabilityRegistry {
    fn default() -> Self { Self::new() }
}
