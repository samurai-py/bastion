use crate::capability::approval::{ApprovalOutcome, ApprovalQueue};
use crate::memory::PrivacyTier;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Invocation context — resolved BEFORE entering registry.invoke.
pub struct InvokeCtx {
    pub owner: String,
    pub privacy_tier: Option<PrivacyTier>,
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

    /// Whether this capability requires explicit owner approval before it may
    /// dispatch (SEC-01 — irreversible/destructive actions).
    ///
    /// SECURITY: exactly like `is_local()`, this is a TYPED property of the
    /// capability itself, decided by whoever implements it — never derived
    /// from a runtime flag passed in by the caller (the removed
    /// `InvokeCtx.needs_approval` was dead scaffolding: hardcoded `false` at
    /// every construction site, never actually set `true` by any caller). The
    /// default is `false` — the overwhelming majority of capabilities are
    /// unaffected by the approval gate.
    fn needs_approval(&self) -> bool {
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
    /// The real SEC-01 approval gate backing store. `None` means no queue is
    /// wired — see `invoke()`'s Policy 2 for why that is a fail-closed deny,
    /// never a silent allow, for any capability with `needs_approval()==true`.
    approval_queue: Option<Arc<ApprovalQueue>>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
            approval_queue: None,
        }
    }

    /// Wire a real `ApprovalQueue` so Policy 2 can actually queue/idempotent-resume
    /// instead of fail-closed-denying every `needs_approval()==true` capability.
    pub fn with_approval_queue(mut self, queue: Arc<ApprovalQueue>) -> Self {
        self.approval_queue = Some(queue);
        self
    }

    /// Plan 11-04: read access to the wired `ApprovalQueue` (if any) — lets
    /// `AgentLoop::run_turn_for`'s pre-LLM approval-resolution intercept check
    /// `pending_for_owner`/`approve`/`reject` WITHOUT going through `invoke()`
    /// (there is no capability to invoke yet at that point — resolution decides
    /// whether to dispatch one). Returns `None` when no queue is wired, exactly
    /// mirroring Policy 2's own fail-closed treatment of an unwired registry.
    pub fn approval_queue(&self) -> Option<&Arc<ApprovalQueue>> {
        self.approval_queue.as_ref()
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
    /// 2. Approval gate (SEC-01) — if `cap.needs_approval()`, gate on the real
    ///    `ApprovalQueue` (queue/idempotent-resume/cache), or fail-closed deny if
    ///    no queue is wired
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

        // Policy 2: approval gate (SEC-01). `needs_approval()` is the SOLE decision
        // source — a typed property of the capability itself, never a caller-supplied
        // flag (T-11-02-01: the removed `InvokeCtx.needs_approval` was exactly that
        // kind of unwired, trust-me flag, and is gone, not left dead alongside this).
        if cap.needs_approval() {
            return match &self.approval_queue {
                // No queue wired: preserve the ORIGINAL fail-closed behavior. A
                // capability requiring approval is unusable (denied) until a queue
                // is attached — never silently allowed just because the gate isn't
                // wired (T-11-02-04, e.g. the Reflector's minimal registry).
                None => {
                    anyhow::bail!(
                        "capability '{}' requires approval but no approval queue is wired — denying fail-closed",
                        name
                    );
                }
                Some(queue) => {
                    let outcome = queue.enqueue_or_reuse(&ctx.owner, name, &args).await?;
                    match outcome {
                        // D-03 idempotent-resume: already ran to completion — return the
                        // cached result, never re-dispatch.
                        ApprovalOutcome::AlreadyExecuted(cached) => Ok(cached),
                        // Not yet approved (freshly queued or still pending): the
                        // capability has NOT run — Dispatch below is structurally
                        // unreachable from this branch (T-11-02-02).
                        ApprovalOutcome::AlreadyPending | ApprovalOutcome::NewlyQueued(_) => {
                            Ok(serde_json::json!({
                                "awaiting_approval": true,
                                "capability": name,
                            }))
                        }
                        // Approved but not yet executed: this invoke() call IS the
                        // resolution (triggered by Plan 11-04's NL intercept) — dispatch
                        // now and record the result for future idempotent-resume.
                        ApprovalOutcome::ApprovedPendingExecution(id) => {
                            let result = cap.invoke(args, ctx).await?;
                            queue.record_executed(id, &result).await?;
                            Ok(result)
                        }
                    }
                }
            };
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

    // --- Plan 11-02 (SEC-01): approval gate ------------------------------------

    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Stub capability with a configurable `needs_approval()` and a call
    /// counter — proves whether the underlying `invoke()` actually dispatched.
    struct ApprovalStubCap {
        name: String,
        schema: Value,
        approval_required: bool,
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Capability for ApprovalStubCap {
        fn name(&self) -> &str {
            &self.name
        }
        fn description(&self) -> &str {
            "approval stub"
        }
        fn input_schema(&self) -> &Value {
            &self.schema
        }
        async fn invoke(&self, _args: Value, _ctx: &InvokeCtx) -> anyhow::Result<Value> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(serde_json::json!({"dispatched": true}))
        }
        fn needs_approval(&self) -> bool {
            self.approval_required
        }
    }

    /// SEC-01 approval-gate tests exercise Policy 2 — they must clear Policy 1
    /// (egress) first. `None` is deny-on-ambiguity fail-closed (same as
    /// `LocalOnly` for a non-local stub), which would block these tests before
    /// the approval gate is ever reached; `CloudOk` always clears Policy 1
    /// (`check_egress`) so the assertions below actually test Policy 2.
    fn ctx_for(owner: &str) -> InvokeCtx {
        InvokeCtx {
            owner: owner.to_string(),
            privacy_tier: Some(PrivacyTier::CloudOk),
        }
    }

    /// Registry wired with a real ApprovalQueue (temp sqlite db) plus one
    /// `needs_approval()==true` capability registered under "dangerous_action".
    async fn make_queue_registry() -> (
        tempfile::NamedTempFile,
        CapabilityRegistry,
        Arc<ApprovalQueue>,
        Arc<AtomicUsize>,
    ) {
        let f = tempfile::NamedTempFile::new().unwrap();
        let path = f.path().to_str().unwrap().to_owned();
        crate::session::SessionManager::new(&path)
            .init_schema()
            .await
            .expect("init_schema");
        let queue = Arc::new(ApprovalQueue::new(path));
        let calls = Arc::new(AtomicUsize::new(0));
        let mut registry = CapabilityRegistry::new().with_approval_queue(queue.clone());
        registry
            .register(Arc::new(ApprovalStubCap {
                name: "dangerous_action".to_string(),
                schema: serde_json::json!({}),
                approval_required: true,
                calls: calls.clone(),
            }))
            .unwrap();
        (f, registry, queue, calls)
    }

    #[tokio::test]
    async fn needs_approval_true_without_queue_fails_closed() {
        let mut registry = CapabilityRegistry::new();
        let calls = Arc::new(AtomicUsize::new(0));
        registry
            .register(Arc::new(ApprovalStubCap {
                name: "dangerous_action".to_string(),
                schema: serde_json::json!({}),
                approval_required: true,
                calls: calls.clone(),
            }))
            .unwrap();

        let result = registry
            .invoke("dangerous_action", serde_json::json!({}), &ctx_for("alice"))
            .await;
        assert!(
            result.is_err(),
            "no queue wired must fail-closed deny, never silently dispatch"
        );
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "must never dispatch when denied"
        );
    }

    #[tokio::test]
    async fn needs_approval_true_with_queue_queues_instead_of_dispatching() {
        let (_f, registry, _queue, calls) = make_queue_registry().await;

        let result = registry
            .invoke(
                "dangerous_action",
                serde_json::json!({"x": 1}),
                &ctx_for("alice"),
            )
            .await
            .expect("first invoke must succeed with an awaiting-approval signal");

        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "must not dispatch on first call"
        );
        assert_eq!(result["awaiting_approval"], serde_json::json!(true));
    }

    #[tokio::test]
    async fn needs_approval_true_dispatches_after_approval_and_records_executed() {
        let (_f, registry, queue, calls) = make_queue_registry().await;
        let args = serde_json::json!({"x": 1});

        registry
            .invoke("dangerous_action", args.clone(), &ctx_for("alice"))
            .await
            .expect("first invoke queues");
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        let pending = queue
            .pending_for_owner("alice")
            .await
            .expect("pending_for_owner");
        assert_eq!(pending.len(), 1);
        let id = pending[0].id;
        queue.approve("alice", id).await.expect("approve");

        let result = registry
            .invoke("dangerous_action", args, &ctx_for("alice"))
            .await
            .expect("second invoke after approval must dispatch");

        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "must dispatch exactly once after approval"
        );
        assert_eq!(result, serde_json::json!({"dispatched": true}));

        let still_pending = queue
            .pending_for_owner("alice")
            .await
            .expect("pending_for_owner 2");
        assert!(
            still_pending.is_empty(),
            "row must no longer be pending after execution (record_executed ran)"
        );
    }

    #[tokio::test]
    async fn needs_approval_false_default_dispatches_immediately_unaffected() {
        let mut registry = CapabilityRegistry::new();
        let calls = Arc::new(AtomicUsize::new(0));
        registry
            .register(Arc::new(ApprovalStubCap {
                name: "safe_action".to_string(),
                schema: serde_json::json!({}),
                approval_required: false,
                calls: calls.clone(),
            }))
            .unwrap();

        let result = registry
            .invoke("safe_action", serde_json::json!({}), &ctx_for("alice"))
            .await
            .expect("default needs_approval()==false must dispatch immediately");

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(result, serde_json::json!({"dispatched": true}));
    }
}
