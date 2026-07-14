//! `embedded-host` — the "second consumer" seam (docs/revamp/BACKLOG.md M5,
//! `docs/revamp/M1-ADR-substrate-split.md`), built ONLY from substrate crates
//! (`bastion-types`, `bastion-runtime`, `bastion-memory`) — never the product
//! package `bastion`.
//!
//! Demonstrates the three things an embedding host actually needs from the
//! public API:
//!
//! 1. **Opaque context injection** — a custom `TurnContextProvider` adds a
//!    block the kernel concatenates without interpreting (SEAM #2).
//! 2. **A custom capability, registered through the public
//!    `CapabilityRegistry` API** — no fork of the registry, no product code.
//! 3. **An authorization policy that denies an action** — the host marks its
//!    capability `needs_approval() == true` (a typed property) and rejects
//!    the queued row through the real `ApprovalQueue::reject`. Writing this
//!    surfaced a REAL API gap: the denial never reaches `invoke()`'s caller
//!    as an `Err` (typed or otherwise) — see the `KNOWN API GAP` comment on
//!    `demonstrate_denied_capability` below, the most valuable output of
//!    this example.
//!
//! Fully offline (mock provider, temp-dir SQLite). `cargo run -p
//! embedded-host` exits 0.

use std::sync::Arc;

use async_trait::async_trait;
use bastion_memory::sqlite::SqliteMemory;
use bastion_memory::{Memory, SharedMemory};
use bastion_runtime::agent::context::{ContextBlock, TurnContextProvider};
use bastion_runtime::agent::loop_::{AgentLoop, DEFAULT_OWNER};
use bastion_runtime::agent::ports::{
    FailureSink, ProviderResolver, RespondOutcome, Responder, ToolSource, TurnContext,
};
use bastion_runtime::capability::approval::SqliteApprovalGate;
use bastion_runtime::capability::{Capability, InvokeCtx};
use bastion_runtime::memory::PrivacyTier;
use bastion_runtime::provider::{Provider, SharedProvider};
use bastion_runtime::session::SessionManager;
use bastion_runtime::types::{CallConfig, LlmResponse, Message, TokenUsage};
use bastion_types::FailureKind;
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// 1. Opaque context injection (SEAM #2).
// ---------------------------------------------------------------------------

/// Stands in for an embedding host's own authoritative context (e.g. "the
/// active support ticket", "the object the operator is looking at"). The
/// kernel concatenates `content` into the system prompt VERBATIM — it never
/// parses or interprets it (invariant #8, `docs/SECURITY-INVARIANTS.md`).
struct HostObjectContextProvider;

#[async_trait]
impl TurnContextProvider for HostObjectContextProvider {
    async fn context_for_turn(
        &self,
        _owner: &str,
        _turn_msg: &str,
        _persona: Option<&str>,
    ) -> Vec<ContextBlock> {
        vec![ContextBlock {
            content: "<host_object id=\"ticket-42\">status=open, priority=high</host_object>"
                .to_string(),
            // CloudOk: this embedded host has decided this particular object
            // summary is safe to send to a cloud-backed provider. A real host
            // would derive this per-object, not hardcode it.
            max_tier: PrivacyTier::CloudOk,
        }]
    }
}

// ---------------------------------------------------------------------------
// 2. A custom capability, registered through the public API.
// ---------------------------------------------------------------------------

/// An irreversible, host-defined action. `needs_approval() -> true` is a
/// TYPED property of the capability itself (never a caller-supplied flag —
/// `docs/SECURITY-INVARIANTS.md` invariant #4), decided here by whoever wrote
/// this capability, exactly the way a real embedding host would mark its own
/// dangerous actions.
struct WireTransferCapability;

#[async_trait]
impl Capability for WireTransferCapability {
    fn name(&self) -> &str {
        "wire_transfer"
    }

    fn description(&self) -> &str {
        "Host-defined irreversible action (embedded-host example)"
    }

    fn input_schema(&self) -> &serde_json::Value {
        static SCHEMA: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
        SCHEMA.get_or_init(|| serde_json::json!({"type": "object"}))
    }

    fn needs_approval(&self) -> bool {
        true
    }

    async fn invoke(
        &self,
        args: serde_json::Value,
        _ctx: &InvokeCtx,
    ) -> anyhow::Result<serde_json::Value> {
        // Never reached in this example — the action is queued by Policy 2 and
        // then rejected, so dispatch never happens
        // (see `demonstrate_denied_capability`).
        Ok(serde_json::json!({"transferred": args}))
    }
}

// ---------------------------------------------------------------------------
// Minimal turn plumbing (same shape as the `minimal-agent` example — kept
// here rather than shared, so each example is readable standalone).
// ---------------------------------------------------------------------------

struct MockProvider;

#[async_trait]
impl Provider for MockProvider {
    async fn complete(
        &self,
        _messages: &[Message],
        _config: &CallConfig,
    ) -> anyhow::Result<LlmResponse> {
        Ok(LlmResponse {
            text: "Hello from embedded-host!".to_string(),
            tool_calls: None,
            usage: TokenUsage::default(),
        })
    }

    async fn complete_simple(&self, prompt: &str) -> anyhow::Result<String> {
        Ok(format!("Hello from embedded-host! (you said: {prompt})"))
    }

    fn context_limit(&self) -> usize {
        1_000_000
    }

    fn model_name(&self) -> &str {
        "mock-minimal"
    }

    fn name(&self) -> &'static str {
        "mock"
    }
}

struct EchoResponder;

#[async_trait]
impl Responder for EchoResponder {
    async fn respond(&self, turn: TurnContext<'_>) -> anyhow::Result<RespondOutcome> {
        let text = turn
            .provider
            .read()
            .await
            .complete_simple(turn.user_input)
            .await?;
        Ok(RespondOutcome {
            text,
            attribution: vec!["embedded-host".to_string()],
            turn_tier: Some(PrivacyTier::CloudOk),
        })
    }
}

struct NoTools;

#[async_trait]
impl ToolSource for NoTools {
    async fn tool_defs(&self) -> anyhow::Result<Vec<serde_json::Value>> {
        Ok(vec![])
    }

    async fn call_tool_with_timeout(
        &self,
        name: &str,
        _args: serde_json::Value,
        _owner: &str,
        _resolved_tier: Option<PrivacyTier>,
    ) -> anyhow::Result<serde_json::Value> {
        anyhow::bail!("embedded-host example registers no external tools (requested: {name})")
    }
}

struct NoopFailureSink;

impl FailureSink for NoopFailureSink {
    fn record_failure(&self, _kind: FailureKind, _tier: Option<PrivacyTier>, _detail: &str) {}
}

struct UnusedResolver;

impl ProviderResolver for UnusedResolver {
    fn resolve(&self, model: &str) -> anyhow::Result<Box<dyn Provider>> {
        anyhow::bail!(
            "embedded-host example never resolves a provider by name (requested: {model})"
        )
    }
}

/// Proves the opaque context block from `HostObjectContextProvider` reaches
/// the system prompt byte-identical — the kernel's SEAM #2 contract.
async fn demonstrate_opaque_context(agent: &AgentLoop) {
    let parts = agent
        .build_system_prompt_parts(DEFAULT_OWNER, "hello", None)
        .await;
    let full_prompt = parts.join("\n\n");
    assert!(
        full_prompt
            .contains("<host_object id=\"ticket-42\">status=open, priority=high</host_object>"),
        "the host's opaque context block must reach the system prompt verbatim"
    );
    println!("[1/3] opaque context block reached the system prompt verbatim — OK");
}

/// Proves the host's own capability is reachable through the SAME public
/// `CapabilityRegistry::invoke` every kernel-internal capability uses — no
/// forked dispatch path. Then tries to express the host's authorization
/// POLICY of denying this specific action.
///
/// KNOWN API GAP (M3 finding — this is the most valuable output of this
/// example, report it precisely): the task this example was written for
/// asked for "a custom approval policy that denies an action and shows a
/// typed `Err`". That is not buildable against today's public API, for two
/// separate reasons discovered while writing this function:
///
/// 1. **No opt-out, so "no queue" isn't reachable through `AgentLoop` at
///    all.** `AgentLoop::new` (`crates/bastion-runtime/src/agent/loop_.rs`)
///    unconditionally constructs `CapabilityRegistry::new().with_approval_queue(
///    Arc::new(ApprovalQueue::new(db_path)))` — there is no constructor
///    parameter to opt out or inject an alternative. So `agent.capability_registry`
///    ALWAYS has a live queue; a host using `AgentLoop` (the only public entry
///    point for a full turn) can never reach the fail-closed `None => bail!`
///    branch of Policy 2 that denies unconditionally when no queue is wired
///    (that branch only fires against a bare, standalone `CapabilityRegistry::new()`
///    built outside `AgentLoop` entirely — not what an embedding host that
///    wants a full turn would build).
/// 2. **Given a queue always exists, there is no injectable decision port
///    either** — `ApprovalQueue` (`crates/bastion-runtime/src/capability/approval.rs`)
///    is a concrete SQLite-backed struct, not a trait. A host's only lever is
///    to call the real `.reject(owner, id)` on a pending row. This function
///    does exactly that — and the result is the actual gap: calling
///    `invoke()` again for the SAME (owner, capability, args) after an
///    explicit reject does **not** return `Err`. `outcome_for_existing_row`
///    (`approval.rs`) maps a `Rejected` row to `ApprovalOutcome::AlreadyPending`
///    — bit-for-bit the same outcome as a still-undecided row — so a
///    rejected action is indistinguishable from a pending one to `invoke()`'s
///    caller. There is no `ApprovalOutcome::Rejected` surfaced at all, typed
///    or otherwise. An embedding host cannot express (or observe) "this
///    action was denied" through this API today — only "not yet approved".
async fn demonstrate_denied_capability(agent: &mut AgentLoop) {
    agent
        .capability_registry
        .register(Arc::new(WireTransferCapability))
        .expect("register wire_transfer");

    let ctx = InvokeCtx {
        owner: DEFAULT_OWNER.to_string(),
        privacy_tier: Some(PrivacyTier::CloudOk),
    };
    let args = serde_json::json!({"amount": 100});

    // First call: AgentLoop's always-wired queue enqueues it — Ok, not Err.
    let first = agent
        .capability_registry
        .invoke("wire_transfer", args.clone(), &ctx)
        .await
        .expect("a freshly wired queue enqueues, it does not error");
    assert_eq!(
        first.data["awaiting_approval"],
        serde_json::json!(true),
        "unexpected first-call shape: {:?}",
        first.data
    );

    // The host's authorization policy decides to DENY this specific action
    // (e.g. "amount over threshold") via the only lever available: reject
    // the now-pending row on the queue `AgentLoop` already wired.
    let queue = agent.capability_registry.approval_gate().clone();
    let pending = queue
        .pending_for_owner(DEFAULT_OWNER)
        .await
        .expect("read pending rows");
    let row = pending
        .iter()
        .find(|r| r.capability_name == "wire_transfer")
        .expect("the row we just enqueued must be pending");
    queue
        .reject(DEFAULT_OWNER, row.id)
        .await
        .expect("reject the pending row");

    // THE GAP: invoking the SAME action again after an explicit reject does
    // NOT return Err — it returns the identical Ok(awaiting_approval: true)
    // as before the reject. A host cannot observe "denied" through invoke().
    let second = agent
        .capability_registry
        .invoke("wire_transfer", args, &ctx)
        .await
        .expect("this does not error today — that IS the gap");
    assert_eq!(
        second.data["awaiting_approval"],
        serde_json::json!(true),
        "if this ever changes, the API gap documented above has been closed \
         upstream — update this example and its doc comment"
    );
    println!(
        "[2/3] wire_transfer rejected via ApprovalQueue::reject, but a subsequent \
         invoke() still returns Ok({:?}) — no typed (or untyped) Err reaches the \
         caller for a denied action; see the API gap documented above",
        second.data
    );
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let db_path = dir
        .path()
        .join("embedded-host.sqlite3")
        .to_str()
        .expect("temp path is valid UTF-8")
        .to_string();

    let session = SessionManager::new(db_path.clone());
    session.init_schema().await?;
    let session_id = session.create_session().await?;

    let memory: SharedMemory = Arc::new(RwLock::new(
        Box::new(SqliteMemory::new(&db_path)) as Box<dyn Memory>
    ));
    let provider: SharedProvider =
        Arc::new(RwLock::new(Box::new(MockProvider) as Box<dyn Provider>));

    let mut agent = AgentLoop::new(
        provider,
        SessionManager::new(db_path.clone()),
        Arc::new(NoTools),
        session_id,
        1.0,
        Arc::new(EchoResponder),
        memory,
        None,
        vec![],
        Arc::new(SqliteApprovalGate::new(db_path.clone())),
        Arc::new(NoopFailureSink),
        // The seam a second consumer uses to inject its own authoritative
        // context — no patch to the kernel, just a `Box<dyn TurnContextProvider>`.
        vec![Box::new(HostObjectContextProvider)],
        Arc::new(UnusedResolver),
        None,
        None,
    );

    demonstrate_opaque_context(&agent).await;
    demonstrate_denied_capability(&mut agent).await;

    let reply = agent.run_turn_for("hello", DEFAULT_OWNER).await?;
    println!("[3/3] full turn completed with the host's context/capability wired in: {reply}");

    Ok(())
}
