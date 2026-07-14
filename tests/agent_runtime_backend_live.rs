//! Ciclo 2.4 — A-06 live proof
//! (`docs/revamp/C2-backend-profile-design.md` §4): one turn of conversation
//! entirely served by `AcpxAgentRuntime`→Claude Code, through the REAL
//! daemon path (`AgentLoop::run_turn_for`, not just adapter-level
//! conformance). Placar em `docs/revamp/A-06-A-07-live.md`.
//!
//! Not run by default (`cargo test`): spawns a real `acpx`/`claude`
//! subprocess, costs real tokens, and depends on host state (`acpx` +
//! `claude` installed, `claude` already authenticated). Run manually:
//!
//! ```text
//! cargo test --test agent_runtime_backend_live -- --ignored --nocapture
//! ```
//!
//! Fixture note: this integration test binary cannot `use` the fixture
//! helpers in `tests/agent_loop_public.rs` (each file under `tests/` compiles
//! as its own separate crate) — the minimal `make_loop` below is a
//! deliberate, small duplication of that file's fixture shape, not a design
//! choice specific to this test.

use bastion_agent_runtime::acpx::AcpxAgentRuntime;
use bastion_agent_runtime::AgentRuntime as _;
use bastion_cognition::goal::{GoalEngine, ScoringConfig};
use bastion_memory::sqlite::SqliteMemory;
use bastion_memory::SharedMemory;
use bastion_personas::persona::{PersonaRegistry, PersonaResponder};
use bastion_providers::{Provider, SharedProvider};
use bastion_runtime::agent::backend::{BackendProfile, ConversationBackend, RuntimeRegistry};
use bastion_runtime::agent::loop_::AgentLoop;
use bastion_runtime::capability::approval::SqliteApprovalGate;
use bastion_runtime::session::SessionManager;
use bastion_types::{CallConfig, LlmResponse, Message, Role};
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::sync::RwLock;

/// Never actually dispatched (the runtime-backed path bypasses `self.provider`
/// entirely) — only present because `AgentLoop::new` requires one.
struct UnusedProvider;

#[async_trait::async_trait]
impl Provider for UnusedProvider {
    async fn complete(&self, _: &[Message], _: &CallConfig) -> anyhow::Result<LlmResponse> {
        anyhow::bail!("UnusedProvider must never be called on the runtime-backed path")
    }
    async fn complete_simple(&self, _prompt: &str) -> anyhow::Result<String> {
        anyhow::bail!("UnusedProvider must never be called on the runtime-backed path")
    }
    fn context_limit(&self) -> usize {
        8192
    }
    fn model_name(&self) -> &str {
        "unused"
    }
    fn name(&self) -> &'static str {
        "unused"
    }
}

fn make_unused_provider() -> SharedProvider {
    Arc::new(RwLock::new(Box::new(UnusedProvider) as Box<dyn Provider>))
}

async fn make_loop(db_path: &str) -> AgentLoop {
    let session = SessionManager::new(db_path);
    session.init_schema().await.expect("init_schema");
    let session_id = session.create_session().await.expect("create_session");

    let memory: SharedMemory = Arc::new(RwLock::new(
        Box::new(SqliteMemory::new(db_path)) as Box<dyn bastion_memory::Memory>
    ));

    let mcp = Arc::new(
        bastion_mcp::McpClient::connect_all("nonexistent_mcp.json")
            .await
            .expect("connect_all empty"),
    );

    AgentLoop::new(
        make_unused_provider(),
        session,
        Arc::new(bastion_mcp::McpToolSource::new(mcp)),
        session_id,
        10.0,
        Arc::new(PersonaResponder::new(PersonaRegistry::new_from_map(
            Default::default(),
        ))),
        memory.clone(),
        Some(Arc::new(GoalEngine::new(db_path, ScoringConfig::default()))),
        vec![],
        Arc::new(SqliteApprovalGate::new(db_path)),
        Arc::new(bastion_cognition::eval::failure_sink::EvalFailureSink),
        bastion::agent::default_context_providers(&memory),
        Arc::new(bastion_providers::registry::RegistryProviderResolver),
        Some(Arc::new(bastion_cognition::agent::dream::DreamFlush::new(
            memory.clone(),
        ))),
        Some(Arc::new(bastion::agent::skills::SkillReloadObserver)),
    )
}

const A06_MARKER: &str = "BASTION-A06-OK";

#[tokio::test]
#[ignore = "spawns real acpx+claude subprocesses, costs tokens; run manually with --ignored"]
async fn a06_runtime_backed_conversation_live() {
    let f = NamedTempFile::new().unwrap();
    let db_path = f.path().to_str().unwrap().to_owned();
    let mut agent = make_loop(&db_path).await;

    let acpx = AcpxAgentRuntime::new("claude").expect("acpx on PATH");
    let health = acpx.health().await.expect("health probe");
    eprintln!("health: {health:?}");
    assert!(health.ready, "acpx/claude not ready: {health:?}");

    let mut registry = RuntimeRegistry::new();
    registry.register(Arc::new(acpx));

    agent = agent
        .with_backend_profile(BackendProfile {
            conversation: ConversationBackend::Runtime("acpx_claude".to_string()),
            ..Default::default()
        })
        .with_runtime_registry(registry);

    let owner = "a06-live-owner";
    let response = agent
        .run_turn_for(
            &format!("Reply with exactly this and nothing else: {A06_MARKER}"),
            owner,
        )
        .await
        .expect("runtime-backed turn must succeed end-to-end through the daemon path");
    eprintln!("response: {response:?}");

    assert!(
        response.contains(A06_MARKER),
        "expected the marker word in the runtime-backed response (proves the response \
         actually came back through AgentLoop::run_turn_for, not a stub), got: {response:?}"
    );

    // "memória grava a resposta" (design doc §3): the assistant response is
    // persisted to the Bastion session — same conversation record the Model
    // path writes, even though the harness owned this turn's tool-loop.
    let session_id = agent
        .session
        .load_most_recent_id_for(owner)
        .await
        .expect("load session id")
        .expect("a session must exist for this owner after the turn");
    let history = agent
        .session
        .load_recent(&session_id)
        .await
        .expect("load history");
    let last_assistant_has_marker = history.iter().rev().any(|m| {
        m.role == Role::Assistant
            && matches!(&m.content, bastion_types::MessageContent::Text(t) if t.contains(A06_MARKER))
    });
    assert!(
        last_assistant_has_marker,
        "the runtime-backed response must be persisted to session history, got: {history:?}"
    );
}
