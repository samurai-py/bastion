//! Kernel ports (M2 step 3a): trait seams that let [`crate::agent::loop_::AgentLoop`]
//! depend on abstract capabilities instead of concrete product/cognition types.
//!
//! This is the in-monolith half of the M2 substrate split
//! (`docs/revamp/M2-ports-design.md`): the traits below are introduced and the
//! loop is wired to depend on them, but no file moves crate yet — that is a
//! separate step (3b). Behavior is unchanged; only the seam is added.

use bastion_types::FailureKind;

use crate::memory::PrivacyTier;

/// P2 — failure telemetry sink.
///
/// Absorbs `eval::capture::record_failure` (`src/eval/capture.rs`), called
/// from the loop's egress-reject path (`agent/loop_.rs`) and from
/// `hooks::output_validator`'s NL-contestation-revoke path (HOOK-03). Resolves
/// the ADR's V4 anomaly (`runtime → cognition` via `hooks/output_validator.rs`
/// using `crate::eval`): both call sites now depend on this trait instead of
/// naming the `eval` module directly.
pub trait FailureSink: Send + Sync {
    /// Record one production-failure signal (EVAL-01).
    ///
    /// Must never panic and never propagate an error — mirrors
    /// `eval::capture::record_failure`'s swallow-on-write-failure contract.
    /// `tier` is the resolved `PrivacyTier` of the turn/belief that failed
    /// (deny-on-ambiguity routing lives in the concrete implementation).
    /// `detail` is a fixed, hardcoded `structural_property` label chosen by
    /// the calling code — never derived from user input (Pitfall 1).
    fn record_failure(&self, kind: FailureKind, tier: Option<PrivacyTier>, detail: &str);
}

/// P3 — external tool catalog.
///
/// Absorbs `McpClient` as a loop field. The M2-ports-design.md sketch scopes
/// this to `tool_defs()` alone (tool *invocation* already flows through
/// `CapabilityRegistry::invoke`, BIG-1, unchanged). In practice `loop_.rs` has
/// two registry-bypass call sites (`dispatch_tool_loop`'s empty-registry
/// fallback, and `run_provider_fallback`'s whole tool-dispatch loop) that call
/// `McpClient::call_tool_with_timeout` directly — these predate this port and
/// are a deliberate escape hatch (registry-bypass safety net), not covered by
/// `CapabilityRegistry::invoke`. To let the `mcp: Arc<McpClient>` FIELD leave
/// the struct entirely (as the design calls for) without changing that
/// behavior, this trait's surface is widened to also cover invocation — a
/// documented divergence from the minimal sketch, not a silent one.
#[async_trait::async_trait]
pub trait ToolSource: Send + Sync {
    /// Anthropic-format tool definitions to offer the model this turn, built
    /// from the MCP registry (name/description/input_schema). Used only by
    /// `run_provider_fallback`'s `CallConfig.tools` — the normal path sources
    /// tool defs from `CapabilityRegistry::list_tool_defs` instead.
    async fn tool_defs(&self) -> anyhow::Result<Vec<serde_json::Value>>;

    /// Registry-bypass tool invocation — mirrors `McpClient::call_tool_with_timeout`
    /// exactly (timeout, Composio bounded retry). Callers apply their own
    /// egress gate (`hooks::egress::check_egress`) before invoking; this trait
    /// does not gate anything itself.
    async fn call_tool_with_timeout(
        &self,
        name: &str,
        args: serde_json::Value,
        owner: &str,
    ) -> anyhow::Result<serde_json::Value>;
}

/// P4 — optional goal-engine port.
///
/// `GoalEngine` becomes a trait object injected as `Option<Arc<dyn GoalPort>>`
/// with exactly the surface the loop uses: `list_goals`, called from the
/// `/goals` and `/drift` cockpit commands (`cockpit_command`,
/// `agent/loop_.rs`). No other `GoalEngine` method is reachable from the loop
/// (confirmed by reading every call site beyond the import, M2-05).
#[async_trait::async_trait]
pub trait GoalPort: Send + Sync {
    /// Return all goals for `owner_id`.
    async fn list_goals(&self, owner_id: &str) -> anyhow::Result<Vec<crate::goal::Goal>>;
}
