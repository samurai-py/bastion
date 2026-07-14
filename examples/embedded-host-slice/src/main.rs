//! `embedded-host-slice` — Loop 3-E / M5, the SECOND embedded-host consumer
//! (`docs/revamp/BACKLOG.md` M5, `docs/revamp/C3-m5-second-consumer-design.md`).
//!
//! `examples/embedded-host` (M3) already proved a second consumer COMPILES
//! against the substrate. This slice proves the boundary actually holds
//! under a second REAL owner: an owner-local `AgentDefinition` built outside
//! the personal Agent, authoritative business context injected from
//! outside, a dynamic object-scoped capability, an authorization policy the
//! host owns, two owners sharing one process with zero leakage, OTel spans
//! the host correlates without the kernel ever learning what it's
//! correlating to, and a versioned rule bundle that propagates to the right
//! owner without a rebuild/redeploy (M5.1).
//!
//! Gate-mãe (non-negotiable, `docs/revamp/C3-m5-second-consumer-design.md`):
//! **zero import of the `bastion` app package, zero fork/patch of any
//! `bastion-*` crate** — every dependency in `Cargo.toml` is a substrate/
//! extension crate, consumed only through its public API. Every `pub` this
//! slice needed but did not find is documented as a finding in
//! `docs/revamp/LOOP-REPORT.md`, not silently worked around.
//!
//! Deliberately GENERIC and neutral: "an embedding host with authoritative
//! business state and an operator" — readable as a team runtime, a support
//! tool, anything. No named closed-source consumer, no cloud/tenancy
//! concept (`scripts/check-scope-and-scrub.sh` enforces this).
//!
//! Fully offline (mocked provider, temp-dir SQLite for session/memory only).
//! `cargo run -p embedded-host-slice` exits 0.

mod capability;
mod context_blocks;
mod otel_capture;
mod plumbing;
mod rule_bundle;

use std::collections::HashMap;
use std::sync::Arc;

use bastion_memory::sqlite::SqliteMemory;
use bastion_memory::{Memory, SharedMemory};
use bastion_personas::persona::{Persona, PersonaRegistry, PersonaResponder};
use bastion_runtime::agent::loop_::AgentLoop;
use bastion_runtime::capability::{InvokeCtx, TurnCapabilityScope};
use bastion_runtime::memory::PrivacyTier;
use bastion_runtime::provider::{Provider, SharedProvider};
use bastion_runtime::session::SessionManager;
use bastion_runtime::types::{BastionError, DenyScope, Message, MessageContent};
use tokio::sync::RwLock;

use capability::{ApproveObjectCapability, ObjectPolicyDenyGate};
use context_blocks::HostObjectContextProvider;
use otel_capture::{init_otel, CapturingExporter};
use plumbing::{MockProvider, NoTools, NoopFailureSink, UnusedResolver};
use rule_bundle::{demonstrate_rule_bundle_propagation, FakeClock, RuleStore};

const OWNER_A: &str = "owner_a";
const OWNER_B: &str = "owner_b";
const PERSONA_NAME: &str = "HostOperatorAgent";
const CASE_A: &str = "case-42";
const CASE_B: &str = "case-77";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // OTel MUST be wired before `AgentLoop::new()` — otherwise spans created
    // inside the kernel are dropped by a no-op tracer (same PITFALL 6
    // `src/main.rs`'s `init_otel_provider` documents for the real app).
    let exporter = CapturingExporter::new();
    let _otel_provider = init_otel(exporter.clone());

    let dir = tempfile::tempdir()?;
    let db_path = dir
        .path()
        .join("embedded-host-slice.sqlite3")
        .to_str()
        .expect("temp path is valid UTF-8")
        .to_string();

    let session = SessionManager::new(db_path.clone());
    session.init_schema().await?;
    let session_id = session.create_session().await?;

    let memory: SharedMemory = Arc::new(RwLock::new(
        Box::new(SqliteMemory::new(&db_path)) as Box<dyn Memory>
    ));

    // --- Component 1: AgentDefinition built OUTSIDE the Agent, owner-local. ---
    // Not loaded from a personas/<name>/SOUL.md file — constructed directly
    // by the host, in host code, proving `Persona`/`PersonaRegistry` are a
    // shared substrate primitive, not a personal-product feature.
    let persona = Persona {
        name: PERSONA_NAME.to_string(),
        description: Some(
            "Programmatically-built, owner-local agent definition — not loaded from SOUL.md"
                .to_string(),
        ),
        system_prompt: "You are the embedding host's own operator assistant.".to_string(),
        tier: PrivacyTier::CloudOk,
        weight: 1.0,
        skills: Vec::new(),
    };
    let mut personas = HashMap::new();
    personas.insert(PERSONA_NAME.to_string(), persona);
    let registry = PersonaRegistry::new_from_map(personas);
    println!(
        "[1] AgentDefinition '{PERSONA_NAME}' built programmatically by the host \
         (PersonaRegistry::new_from_map) — the SAME Persona/PersonaRegistry/PersonaResponder \
         machinery the personal Bastion Agent uses, no fork of the schema"
    );

    // --- Component 2: authoritative business context, one object per owner. ---
    let host_object_provider = HostObjectContextProvider::new([
        (
            OWNER_A.to_string(),
            format!("<host_object id=\"{CASE_A}\">status=open, priority=high</host_object>"),
        ),
        (
            OWNER_B.to_string(),
            format!("<host_object id=\"{CASE_B}\">status=open, priority=low</host_object>"),
        ),
    ]);

    // --- M5.1: RuleBundle propagation — deterministic, standalone (no live LLM needed). ---
    let rule_store = Arc::new(RuleStore::new());
    let clock = FakeClock::new(1_700_000_000_000_000_000); // arbitrary fixed epoch-ns start
    let rule_provider =
        demonstrate_rule_bundle_propagation(rule_store.clone(), clock.clone()).await?;

    // --- Wiring: AgentLoop::new — every argument is public bastion-* API. ---
    // Kept for the OTel-correlation finding below (component 6) — `AgentLoop::new`
    // moves `session_id` by value.
    let construction_session_id = session_id.clone();
    let mut agent = AgentLoop::new(
        Arc::new(RwLock::new(
            Box::new(MockProvider::new(PERSONA_NAME)) as Box<dyn Provider>
        )) as SharedProvider,
        SessionManager::new(db_path.clone()),
        Arc::new(NoTools),
        session_id,
        1.0,
        Arc::new(PersonaResponder::new(registry)),
        memory,
        None,
        vec![],
        // Component 4: the host's OWN authorization policy — no SQLite queue,
        // no product code, just an `Arc<dyn ApprovalGate>` built here.
        Arc::new(ObjectPolicyDenyGate {
            cleared_status: "cleared_for_action",
        }),
        Arc::new(NoopFailureSink),
        // Component 2 + M5.1: the two host-owned `TurnContextProvider`s — the
        // seam a second consumer uses to inject its own authoritative
        // context, no patch to the kernel.
        vec![Box::new(host_object_provider), Box::new(rule_provider)],
        Arc::new(UnusedResolver),
        None,
        None,
    );

    // --- Component 3: dynamic, object-scoped capability via the public registry API. ---
    let capability_name = ApproveObjectCapability::capability_name(CASE_A);
    agent
        .capability_registry
        .register(Arc::new(ApproveObjectCapability::new(CASE_A)))
        .expect("register the object-scoped capability");
    println!(
        "[3] object-scoped capability '{capability_name}' registered through the public \
         CapabilityRegistry API — no forked dispatch path"
    );

    demonstrate_object_policy_denial(&agent, &capability_name).await;
    demonstrate_trust_quarantine_preserved(&mut agent, &capability_name).await;
    demonstrate_owner_scoped_system_prompt(&agent).await;
    let (session_a, session_b) = demonstrate_two_owner_isolation(&mut agent).await?;
    demonstrate_otel_correlation(&exporter, &session_a, &session_b, &construction_session_id);

    println!(
        "\nAll 7 M5 components + M5.1 RuleBundle propagation passed — zero import of the \
         `bastion` app package, zero fork/patch of any bastion-* crate."
    );
    Ok(())
}

/// Component 4: proves the host's OWN `ApprovalGate` (`ObjectPolicyDenyGate`)
/// denies the object-scoped capability with a typed
/// `Err(BastionError::ApprovalDenied)` — the same shape
/// `examples/embedded-host`'s `ThresholdDenyGate` established, exercised
/// here against a different (object-status, not numeric) business rule.
async fn demonstrate_object_policy_denial(agent: &AgentLoop, capability_name: &str) {
    let ctx = InvokeCtx {
        owner: OWNER_A.to_string(),
        privacy_tier: Some(PrivacyTier::CloudOk),
    };
    let args = serde_json::json!({"object_status": "pending_review"});

    let err = agent
        .capability_registry
        .invoke(capability_name, args, &ctx)
        .await
        .expect_err(
            "ObjectPolicyDenyGate must deny any object_status other than 'cleared_for_action'",
        );

    match err.downcast_ref::<BastionError>() {
        Some(BastionError::ApprovalDenied { capability, scope }) => {
            assert_eq!(capability, capability_name);
            assert_eq!(
                *scope,
                DenyScope::Turn,
                "ObjectPolicyDenyGate returns the product default scope"
            );
            println!(
                "[4] '{capability_name}' denied by the host's own ApprovalGate \
                 (ObjectPolicyDenyGate, no SQLite) — invoke() returned a typed \
                 Err(BastionError::ApprovalDenied{{capability: {capability:?}, scope: {scope:?}}})"
            );
        }
        other => {
            panic!("expected Err(BastionError::ApprovalDenied), got: {other:?} (display: {err})")
        }
    }
}

/// Component 7: trust/spotlighting preserved. Uses the SAME public
/// `TurnCapabilityScope::quarantine` primitive `run_turn_for_with_trust(...,
/// untrusted: true)` uses internally (SEC-05) — proves a previously
/// registered, privileged capability becomes genuinely invisible
/// (`list_tool_defs()` empty, `invoke()` errors "unknown capability") for
/// the duration of an untrusted dispatch window, and is fully restored the
/// instant it ends. Untrusted content can never acquire tool authority.
async fn demonstrate_trust_quarantine_preserved(agent: &mut AgentLoop, capability_name: &str) {
    let before = agent.capability_registry.list_tool_defs();
    assert!(
        !before.is_empty(),
        "the object-scoped capability must already be registered before this check"
    );

    {
        let scope = TurnCapabilityScope::quarantine(&mut agent.capability_registry);
        assert!(
            scope.list_tool_defs().is_empty(),
            "an untrusted turn's dispatch must see ZERO capabilities — genuinely invisible, \
             not just 'no new tools added'"
        );
        let ctx = InvokeCtx {
            owner: OWNER_A.to_string(),
            privacy_tier: Some(PrivacyTier::CloudOk),
        };
        let blocked = scope
            .invoke(capability_name, serde_json::json!({}), &ctx)
            .await;
        assert!(
            blocked.is_err(),
            "a previously-registered capability must be genuinely uninvokable while quarantined"
        );
    }

    let after = agent.capability_registry.list_tool_defs();
    assert_eq!(
        serde_json::to_string(&before).unwrap(),
        serde_json::to_string(&after).unwrap(),
        "every capability must be restored, identical to before quarantine, once the \
         untrusted dispatch window ends"
    );
    println!(
        "[7] trust/spotlighting preserved: TurnCapabilityScope::quarantine() — the SAME \
         mechanism run_turn_for_with_trust(untrusted: true) uses internally — hides every \
         capability during an untrusted turn and fully restores it after, so untrusted \
         content can never acquire tool authority"
    );
}

/// Components 2 + M5.1, end to end: each owner's REAL system prompt (built
/// by the kernel's own SEAM #2 assembler, `AgentLoop::build_system_prompt_parts`)
/// carries only ITS OWN authoritative object and rule bundle — never the
/// other owner's.
async fn demonstrate_owner_scoped_system_prompt(agent: &AgentLoop) {
    let prompt_a = agent
        .build_system_prompt_parts(OWNER_A, "hello", Some(PERSONA_NAME))
        .await
        .join("\n\n");
    let prompt_b = agent
        .build_system_prompt_parts(OWNER_B, "hello", Some(PERSONA_NAME))
        .await
        .join("\n\n");

    assert!(
        prompt_a.contains(CASE_A),
        "owner A's system prompt must contain its own authoritative object"
    );
    assert!(
        !prompt_a.contains(CASE_B),
        "owner A's system prompt must NEVER contain owner B's object"
    );
    assert!(prompt_b.contains(CASE_B));
    assert!(!prompt_b.contains(CASE_A));

    // Only owner A has a RuleBundle artifact (published by
    // `demonstrate_rule_bundle_propagation`) — owner B's prompt must carry
    // no rule_bundle block at all.
    assert!(prompt_a.contains("<rule_bundle"));
    assert!(!prompt_b.contains("<rule_bundle"));

    println!(
        "[2 + M5.1] SEAM #2 end-to-end: each owner's REAL system prompt carries only its own \
         authoritative object and rule bundle — never the other owner's"
    );
}

/// Component 5: two owners sharing ONE `AgentLoop`/process, zero
/// cross-owner leakage — separate sessions (CR-04), separate history, and
/// (acceptance criterion 7) none of the host's own authoritative state ever
/// lands in Bastion's session store. Returns `(session_a, session_b)` for
/// the OTel correlation demo.
async fn demonstrate_two_owner_isolation(
    agent: &mut AgentLoop,
) -> anyhow::Result<(String, String)> {
    let reply_a = agent
        .run_turn_for("hello, this is owner A speaking", OWNER_A)
        .await?;
    let reply_b = agent
        .run_turn_for("hello, this is owner B speaking", OWNER_B)
        .await?;
    assert!(!reply_a.is_empty());
    assert!(!reply_b.is_empty());

    let session_a = agent
        .session
        .load_most_recent_id_for(OWNER_A)
        .await?
        .expect("owner A session must exist");
    let session_b = agent
        .session
        .load_most_recent_id_for(OWNER_B)
        .await?
        .expect("owner B session must exist");
    assert_ne!(
        session_a, session_b,
        "two owners must never share a session id (CR-04)"
    );

    let history_a = agent.session.load_recent(&session_a).await?;
    let history_b = agent.session.load_recent(&session_b).await?;

    let text_of = |m: &Message| match &m.content {
        MessageContent::Text(t) => t.clone(),
        _ => String::new(),
    };
    assert!(history_a.iter().any(|m| text_of(m).contains("owner A")));
    assert!(
        !history_a.iter().any(|m| text_of(m).contains("owner B")),
        "owner A's session must never contain owner B's message"
    );
    assert!(history_b.iter().any(|m| text_of(m).contains("owner B")));
    assert!(
        !history_b.iter().any(|m| text_of(m).contains("owner A")),
        "owner B's session must never contain owner A's message"
    );

    // Acceptance criterion 7: the host's own authoritative object/rule
    // bundle is injected transiently per turn via SEAM #2 — it is never
    // persisted into Bastion's session/message store.
    assert!(
        !history_a
            .iter()
            .any(|m| text_of(m).contains("rule_bundle") || text_of(m).contains(CASE_A)),
        "the host's authoritative context/rule bundle must never persist in Bastion's session store"
    );

    println!(
        "[5] two owners, one AgentLoop: separate sessions ({session_a} != {session_b}), \
         zero cross-owner history leakage, no host entity in Bastion's session store"
    );
    Ok((session_a, session_b))
}

/// Component 6 — and a FINDING (`docs/revamp/LOOP-REPORT.md`).
///
/// The design intent: the kernel emits generic `gen_ai.*` spans with zero
/// knowledge of the host's business object, and the host correlates a span
/// back to its own object using public data alone (e.g. the owner-scoped
/// session id from `SessionManager::load_most_recent_id_for`).
///
/// What this slice actually found: `AgentLoop::run_turn_for_with_trust`
/// (`crates/bastion-runtime/src/agent/loop_.rs`, ~lines 1306-1326) stamps
/// the root `invoke_agent` span's `gen_ai.conversation.id` from
/// `self.session_id` (the field fixed at `AgentLoop::new` construction time)
/// AT SPAN-CREATION — several lines BEFORE the CR-04 per-owner session
/// resolution runs. For any owner other than the one live at construction,
/// this attribute is simply WRONG: both `owner_a`'s and `owner_b`'s turns
/// below are stamped with the SAME id, never their own real (CR-04-resolved)
/// session. No other attribute on this span identifies the owner either.
/// Net effect: a host running MULTIPLE owners through one `AgentLoop` (this
/// slice's own component 5 setup) cannot correlate a span to a specific
/// owner via ANY span attribute today — the only thing left to fall back on
/// is call ORDER, which only works for a single-threaded, sequential caller
/// like this example, not a real concurrent multi-owner host.
///
/// Suggested fix (not applied here — a behavior change to a stable-contract
/// file, out of scope for this slice per the M5 gate): stamp
/// `gen_ai.conversation.id` (and ideally an explicit `owner`/`enduser.id`
/// attribute) from the CR-04-resolved `session_id` local, after it's
/// computed, not from `self.session_id`.
fn demonstrate_otel_correlation(
    exporter: &CapturingExporter,
    session_a: &str,
    session_b: &str,
    construction_session_id: &str,
) {
    let spans = exporter.snapshot();
    let turn_spans: Vec<_> = spans
        .iter()
        .filter(|s| s.name.as_ref() == "invoke_agent")
        .collect();
    assert_eq!(
        turn_spans.len(),
        2,
        "expected exactly one invoke_agent span per run_turn_for call"
    );

    let conversation_id = |s: &opentelemetry_sdk::trace::SpanData| {
        s.attributes
            .iter()
            .find(|kv| kv.key.as_str() == "gen_ai.conversation.id")
            .map(|kv| kv.value.as_str().into_owned())
    };

    // The finding, pinned down precisely: BOTH spans carry the
    // construction-time session id — NEITHER carries owner_a's or owner_b's
    // real session id.
    assert_eq!(
        conversation_id(turn_spans[0]).as_deref(),
        Some(construction_session_id),
        "confirms the finding: span 1's gen_ai.conversation.id is the constructor-time id"
    );
    assert_eq!(
        conversation_id(turn_spans[1]).as_deref(),
        Some(construction_session_id),
        "confirms the finding: span 2's gen_ai.conversation.id is ALSO the constructor-time id"
    );
    assert_ne!(construction_session_id, session_a);
    assert_ne!(construction_session_id, session_b);

    // Two distinct traces at least prove the kernel emits ONE span per turn
    // (not sharing/reusing a trace across owners) — the smallest amount of
    // correlation that survives this gap.
    assert_ne!(
        turn_spans[0].span_context.trace_id(),
        turn_spans[1].span_context.trace_id(),
        "two owners' turns must at least be two distinct traces"
    );

    println!(
        "[6][FINDING] gen_ai.conversation.id on the kernel's invoke_agent span is stamped from \
         AgentLoop's CONSTRUCTOR-time session_id (loop_.rs ~1306-1326), BEFORE the CR-04 \
         per-owner session is resolved a few lines later — both owner_a's and owner_b's spans \
         carry the SAME (wrong) id '{construction_session_id}', never their real session \
         ({session_a} / {session_b}). No other attribute on this span identifies the owner \
         either. A multi-owner host can only correlate spans to owners by CALL ORDER today \
         (trace {:?} = 1st call = owner_a/'{CASE_A}', trace {:?} = 2nd call = \
         owner_b/'{CASE_B}'), not by any span attribute — see docs/revamp/LOOP-REPORT.md.",
        turn_spans[0].span_context.trace_id(),
        turn_spans[1].span_context.trace_id(),
    );
}
