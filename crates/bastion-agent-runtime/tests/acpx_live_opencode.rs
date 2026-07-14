//! Live A-02 conformance run of [`bastion_agent_runtime::acpx::AcpxAgentRuntime`]
//! against a real, host-authenticated `opencode` agent via `acpx` (A-05
//! matrix cell: `opencode` via `acpx`).
//!
//! Not run by default (`cargo test`): it spawns real subprocesses and
//! depends on host state (`acpx`/`opencode` installed, `opencode auth login`
//! already done). Run manually:
//!
//! ```text
//! cargo test -p bastion-agent-runtime --test acpx_live_opencode -- --ignored --nocapture
//! ```
//!
//! # Result: blocked by an adapter-level `--auth-policy fail` mismatch, not
//! by missing host auth
//!
//! `opencode auth login` was completed on this host (`opencode auth list`
//! shows `OpenCode Go`/`OpenAI`/`OpenCode Zen` credentials) and the
//! underlying transport genuinely works: a raw, manual
//! `acpx --format json --cwd <tmp> opencode prompt -s <name> "..."` (built-in
//! default flags, no `--auth-policy` override) completes a full turn —
//! `session/resume` → `session/prompt` → `agent_message_chunk` → `Ended`
//! with a real reply, using the host's already-persisted opencode
//! credentials.
//!
//! But [`AcpxAgentRuntime::build_prompt_command`] (`src/acpx.rs`)
//! unconditionally appends `--auth-policy fail` for every wrapped agent.
//! `opencode`'s native ACP server (`opencode acp`, invoked by acpx as
//! `npx -y opencode-ai acp`) advertises ACP `authMethods: [{"id":
//! "opencode-login", ...}]` on `initialize` — acpx tries to match that
//! against its **own** credential store (used for agents whose auth it can
//! broker directly), finds nothing (opencode manages its own
//! `~/.local/share/opencode/auth.json`, a store acpx's matcher does not
//! know about), and with `--auth-policy fail` aborts the whole invocation
//! with a top-level JSON-RPC error before `session/prompt` is ever sent:
//!
//! ```text
//! {"jsonrpc":"2.0","id":null,"error":{"code":-32603,
//!  "message":"agent advertised auth methods [opencode-login] but no
//!  matching credentials found",
//!  "data":{"acpxCode":"RUNTIME","detailCode":"AUTH_REQUIRED","origin":"acp", ...}}}
//! ```
//!
//! Re-running the exact same invocation with `--auth-policy skip` (or no
//! `--auth-policy` flag at all — the CLI default) instead of `fail`
//! completes normally. `claude` is unaffected because acpx spawns it via a
//! **built-in agent bridge**
//! (`@agentclientprotocol/claude-agent-acp`) that does not advertise ACP
//! `authMethods` at all, so the credential-matching/abort branch never
//! triggers for that agent — this is an acpx-side, per-wrapped-agent
//! inconsistency, not something `AcpxAgentRuntime` controls once it has
//! chosen to hardcode `fail`.
//!
//! Because the abort happens as an unsolicited top-level frame (`"id":
//! null`, no `"method"`) rather than a response correlated to our
//! `session/prompt` request, [`crate` `FrameInterpreter`] (private to
//! `src/acpx.rs`) does not recognize it as anything actionable — it silently
//! ignores the frame (no method, no matching id), the acpx process then
//! exits, stdout hits EOF, and `run_prompt_reader` reports the generic
//! `TaskOutcome::Failed { reason: "acpx process ended without a terminal
//! frame (crash or premature exit)" }`, not a `RuntimeError::Unavailable`/auth-
//! specific reason. This is a genuine, reproducible finding, not a fluke —
//! **not fixed here** (out of scope: doing so would mean either making
//! `--auth-policy` configurable per-agent in `AcpxAgentRuntime` or teaching
//! `FrameInterpreter` to recognize this frame shape, both real code changes
//! requiring their own impact analysis/review, not a one-off conformance
//! smoke).
//!
//! The test below runs the real, unmodified `AcpxAgentRuntime` against
//! `opencode` and asserts the *current* documented failure mode, so it acts
//! as a regression lock: if a future change to `--auth-policy` handling (or
//! to acpx itself) makes this start passing, this assertion breaks loudly
//! and A-05 needs updating.
//!
//! # Cost note
//!
//! Because the abort happens before `session/prompt` is ever sent, **no
//! model/LLM tokens are spent** by any check in this file — the whole
//! 14-check sweep is a free, deterministic transport-level failure.

use bastion_agent_runtime::acpx::AcpxAgentRuntime;
use bastion_agent_runtime::conformance::{self, ConformanceScenarios};
use bastion_agent_runtime::*;
use std::collections::BTreeMap;
use std::time::Duration;

fn make_spec(workspace_root: std::path::PathBuf) -> SessionSpec {
    let mut allow = BTreeMap::new();
    if let Ok(home) = std::env::var("HOME") {
        allow.insert("HOME".to_string(), home);
    }
    if let Ok(path) = std::env::var("PATH") {
        allow.insert("PATH".to_string(), path);
    }
    SessionSpec {
        owner: "acpx-live-test".to_string(),
        workspace: WorkspacePolicy {
            root: workspace_root,
            read_only: false,
            deny: Vec::new(),
        },
        sandbox: SandboxProfile::WorkspaceNet,
        permissions: PermissionProfile {
            allow: vec!["*".to_string()],
        },
        auth: AuthProfileRef("host-opencode-login".to_string()),
        runtime_id: "opencode".to_string(),
        timeout: TimeoutPolicy {
            per_task: Duration::from_secs(60),
            idle: Duration::from_secs(120),
        },
        env: EnvPolicy { allow },
        mcp_bridge: None,
        otel: OtelContext::default(),
    }
}

fn make_scenarios() -> ConformanceScenarios {
    ConformanceScenarios {
        happy_path: TaskInput {
            prompt: "Reply with exactly: ok".to_string(),
            attachments: Vec::new(),
            expected: TaskExpectation::Conversation,
        },
        never_terminates: TaskInput {
            prompt: "Count slowly from 1 to 1000000, one number per line, in words, no code."
                .to_string(),
            attachments: Vec::new(),
            expected: TaskExpectation::Conversation,
        },
        requests_permission: TaskInput {
            prompt: "create file permission_probe.txt with content probe".to_string(),
            attachments: Vec::new(),
            expected: TaskExpectation::CodeChange,
        },
        produces_artifact: TaskInput {
            prompt: "create file hello.txt with content hi".to_string(),
            attachments: Vec::new(),
            expected: TaskExpectation::CodeChange,
        },
        // Generous even though the current blocker aborts before any real
        // network/model latency is incurred (see module docs) — kept
        // consistent with the claude live suite in case the block is lifted.
        watchdog: Duration::from_secs(30),
    }
}

/// Reason substring `run_prompt_reader` emits for the current, documented
/// A-05 opencode-via-acpx block: acpx's own `--auth-policy fail` aborts the
/// invocation with a top-level, uncorrelated JSON-RPC error before
/// `session/prompt` is sent, `FrameInterpreter` doesn't recognize that frame
/// shape, and the acpx process exit reads back as a generic premature-exit
/// failure (see module docs for the full mechanism and the raw acpx error).
const EXPECTED_BLOCK_REASON: &str = "acpx process ended without a terminal frame";

#[tokio::test]
#[ignore = "spawns real acpx+opencode subprocesses; run manually with --ignored"]
async fn acpx_opencode_conformance_live() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let spec = make_spec(workspace.path().to_path_buf());
    let scenarios = make_scenarios();
    let runtime = AcpxAgentRuntime::new("opencode").expect("acpx on PATH");

    let health = runtime.health().await.expect("health probe");
    eprintln!("health: {health:?}");
    assert!(health.ready, "acpx not ready: {health:?}");

    let results = conformance::run_all(&runtime, &spec, &scenarios).await;
    let report = conformance::format_report(&results);
    eprintln!("{report}");

    // Documenting assertion, not a "must all pass" gate (unlike the claude
    // suite): happy_path is the clearest, cheapest signal of the known A-05
    // block. If this ever starts passing, the acpx-side auth-policy
    // mismatch (or this adapter's hardcoded `--auth-policy fail`) has
    // changed and A-05 needs updating, not this assertion.
    let happy_path = results
        .iter()
        .find(|(name, _)| *name == "happy_path")
        .map(|(_, r)| r.clone())
        .expect("happy_path present in run_all output");

    match &happy_path {
        conformance::CheckResult::Fail(detail) => {
            assert!(
                detail.contains(EXPECTED_BLOCK_REASON),
                "happy_path failed, but not with the documented A-05 opencode block \
                 reason (acpx --auth-policy fail vs opencode's advertised authMethods) \
                 -- got: {detail}"
            );
        }
        other => panic!(
            "expected happy_path to Fail with the documented A-05 opencode-via-acpx \
             auth-policy block, got {other:?} instead -- if this is now Pass, the block \
             has been lifted (acpx or adapter change) and A-05 §opencode needs updating"
        ),
    }

    // Checks that assert exact-content outcomes (`Ended{Success}` with
    // specific evidence) hit the same transport-level abort and Fail with
    // the same reason. Empirically NOT in this group: `resume`/`steer`
    // (agent-independent, rooted in `descriptor()`, identical to the claude
    // cell), the permission-bridge/fault-injection Skips, and — measured
    // live, not assumed — `cancel_graceful`/`cancel_kill`/`timeout`, which
    // tolerate the task ending in *any* terminal state (their contract is
    // "cancel/timeout doesn't hang", not "the content is Success"), so an
    // immediate `Failed` from the auth-policy abort still satisfies them.
    let live_submit_checks = [
        "happy_path",
        "queue_or_reject",
        "event_ordering_terminal",
        "artifact_digest",
    ];
    for name in live_submit_checks {
        let result = results
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, r)| r.clone())
            .unwrap_or_else(|| panic!("{name} present in run_all output"));
        assert!(
            result.is_fail(),
            "{name}: expected Fail (A-05 opencode block propagates to every live-submit \
             check), got {result:?}"
        );
    }
}
