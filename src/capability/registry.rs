use crate::memory::PrivacyTier;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

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

    /// Whether this capability executes entirely locally (no data leaves the host).
    ///
    /// SECURITY (D-13 guardrail 3): the egress policy keys on THIS typed property,
    /// never on the capability's `name()` string. The default is `false` (treated as
    /// external → LocalOnly tier blocks it, fail-closed). Only adapters that are local
    /// by construction (NlCommandAdapter) override this to `true`. A remote MCP server
    /// cannot opt into the local short-circuit by naming its tool `cmd:*` — locality is
    /// a property of the adapter TYPE, not a forgeable string.
    fn is_local(&self) -> bool {
        false
    }
}

/// Unified capability registry.
///
/// Single policy enforcement point — every frontend (direct fn, MCP tool, NL command)
/// invokes through here. check_egress is called once per invoke at this boundary.
#[derive(Clone)]
pub struct CapabilityRegistry {
    inner: HashMap<String, Arc<dyn Capability>>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    /// Register a capability under its `name()`.
    ///
    /// SECURITY: rejects two impersonation vectors (D-13 guardrail):
    /// 1. A non-local capability claiming the reserved `cmd:` namespace — only
    ///    `is_local()` capabilities (NL commands) may use `cmd:` keys, so a remote
    ///    MCP tool named `cmd:exfil` cannot acquire the local egress short-circuit.
    /// 2. Overwriting an existing key — a later registration cannot shadow/impersonate
    ///    an already-registered built-in capability.
    pub fn register(&mut self, cap: Arc<dyn Capability>) -> anyhow::Result<()> {
        let name = cap.name();
        if name.starts_with("cmd:") && !cap.is_local() {
            anyhow::bail!(
                "capability '{}' uses the reserved 'cmd:' namespace but is not a local NL command — refusing to register",
                name
            );
        }
        if self.inner.contains_key(name) {
            anyhow::bail!(
                "capability '{}' is already registered — refusing to overwrite",
                name
            );
        }
        self.inner.insert(name.to_owned(), cap);
        Ok(())
    }

    pub fn list_names(&self) -> Vec<&str> {
        self.inner.keys().map(|s| s.as_str()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Remove a capability by name. Idempotent — returns false if not present.
    ///
    /// SECURITY: remove does NOT check guardrails (cmd: namespace etc.) because
    /// removal does not create an attack vector — only register() needs to check.
    pub fn remove(&mut self, name: &str) -> bool {
        self.inner.remove(name).is_some()
    }

    /// Return tool definitions in the JSON format expected by the provider
    /// (name/description/input_schema). Compatible with `anthropic_tools_to_openai()`
    /// in openrouter.rs.
    ///
    /// SORTED by capability name (COST-01/D-14b prerequisite): `self.inner` is a
    /// `HashMap`, whose iteration order is unspecified and can shift across an
    /// intervening register+remove cycle (e.g. `TurnCapabilityScope`, above) even
    /// when the surviving capability set is unchanged. Plan 08-10's byte-stable
    /// cache-prefix guarantee requires this listing to serialize identically
    /// turn-over-turn — an unsorted HashMap iteration would silently invalidate
    /// that guarantee.
    pub fn list_tool_defs(&self) -> Vec<serde_json::Value> {
        let mut caps: Vec<&Arc<dyn Capability>> = self.inner.values().collect();
        caps.sort_by(|a, b| a.name().cmp(b.name()));
        caps.into_iter()
            .map(|cap| {
                serde_json::json!({
                    "name": cap.name(),
                    "description": cap.description(),
                    "input_schema": cap.input_schema()
                })
            })
            .collect()
    }

    /// Single policy enforcement point (D-13 non-negotiable guardrail).
    ///
    /// Policy order:
    /// 1. Egress check — fail-closed on LocalOnly or None tier for non-local adapters
    /// 2. Approval gate — if ctx.needs_approval, gate on Phase 3 queue
    /// 3. Dispatch to capability adapter
    pub async fn invoke(&self, name: &str, args: Value, ctx: &InvokeCtx) -> anyhow::Result<Value> {
        let cap = self
            .inner
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("unknown capability: {}", name))?;

        // Policy 1: egress check.
        // Locality is a TYPED property of the adapter (`is_local()`), NEVER derived from
        // the capability name string — a remote MCP server could otherwise forge a `cmd:`
        // name to acquire the local short-circuit (D-13 guardrail 3). Local capabilities
        // (NL commands) map to "ollama" (always passes); everything else maps to "external"
        // so LocalOnly / None tiers are blocked fail-closed.
        let provider_for_policy = if cap.is_local() { "ollama" } else { "external" };
        crate::hooks::egress::check_egress(ctx.privacy_tier, provider_for_policy)?;

        // Policy 2: approval gate. Fail-closed until the Phase 3 approval queue is wired.
        // The documented invariant is "no call path bypasses the approval queue"; logging
        // and proceeding would silently violate it. When the queue lands in Phase 3, replace
        // this bail with the actual await on the queue.
        if ctx.needs_approval {
            anyhow::bail!(
                "capability '{}' requires approval but the approval queue is not yet available (Phase 3) — denying fail-closed",
                name
            );
        }

        // Dispatch
        cap.invoke(args, ctx).await
    }
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard for ephemeral capabilities scoped to a single turn (SEAM #3).
///
/// Registers capabilities on `new()` and removes them on `Drop` — guarantees cleanup
/// even if the turn errors out. Capabilities that fail `register()` are not tracked
/// and will not be removed on drop.
pub struct TurnCapabilityScope<'a> {
    registry: &'a mut CapabilityRegistry,
    registered: Vec<String>,
}

impl<'a> TurnCapabilityScope<'a> {
    /// Create the scope and register capabilities. Registration failures are silently
    /// skipped — those capabilities are not added to `registered` and won't be removed.
    pub fn new(registry: &'a mut CapabilityRegistry, caps: Vec<Arc<dyn Capability>>) -> Self {
        let mut registered = Vec::new();
        for cap in caps {
            let name = cap.name().to_owned();
            if registry.register(cap).is_ok() {
                registered.push(name);
            }
        }
        Self {
            registry,
            registered,
        }
    }
}

impl<'a> Drop for TurnCapabilityScope<'a> {
    fn drop(&mut self) {
        for name in &self.registered {
            self.registry.remove(name);
        }
    }
}

/// Read-only access to the underlying registry while the scope is alive.
///
/// The scope holds the sole `&mut CapabilityRegistry` for its whole lifetime
/// (needed so `Drop` can always remove what it registered, even on early
/// return) — so callers that need to `invoke()` a capability while it is still
/// registered (e.g. Plan 08-03's `complete_structured_via_forced_tool_call`)
/// cannot reborrow the original `&mut` reference. `Deref` exposes the
/// immutable `invoke`/`list_*` surface without weakening that guarantee:
/// nothing here can register/remove a capability out from under the scope.
impl<'a> std::ops::Deref for TurnCapabilityScope<'a> {
    type Target = CapabilityRegistry;

    fn deref(&self) -> &Self::Target {
        self.registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubCap {
        name: String,
        schema: Value,
    }

    #[async_trait]
    impl Capability for StubCap {
        fn name(&self) -> &str {
            &self.name
        }
        fn description(&self) -> &str {
            "stub"
        }
        fn input_schema(&self) -> &Value {
            &self.schema
        }
        async fn invoke(&self, _args: Value, _ctx: &InvokeCtx) -> anyhow::Result<Value> {
            Ok(Value::Null)
        }
    }

    fn stub(name: &str) -> Arc<dyn Capability> {
        Arc::new(StubCap {
            name: name.to_owned(),
            schema: serde_json::json!({}),
        })
    }

    #[test]
    fn list_tool_defs_returns_capabilities_sorted_by_name() {
        let mut registry = CapabilityRegistry::new();
        registry.register(stub("z")).unwrap();
        registry.register(stub("a")).unwrap();
        registry.register(stub("m")).unwrap();

        let names: Vec<String> = registry
            .list_tool_defs()
            .iter()
            .map(|d| d["name"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(names, vec!["a", "m", "z"]);
    }

    #[test]
    fn list_tool_defs_is_byte_stable_across_register_remove_cycle() {
        let mut registry = CapabilityRegistry::new();
        registry.register(stub("z")).unwrap();
        registry.register(stub("a")).unwrap();
        registry.register(stub("m")).unwrap();

        let before = serde_json::to_string(&registry.list_tool_defs()).unwrap();

        // Mirror TurnCapabilityScope: register an ephemeral 4th capability, then drop it.
        {
            let _scope = TurnCapabilityScope::new(&mut registry, vec![stub("ephemeral")]);
        }

        let after = serde_json::to_string(&registry.list_tool_defs()).unwrap();
        assert_eq!(
            before, after,
            "an intervening register+remove cycle must not perturb list_tool_defs() ordering"
        );
    }
}
