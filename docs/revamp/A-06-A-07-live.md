# A-06 / A-07 — live proof scoreboard (Ciclo 2.4)

> Companion to `docs/revamp/C2-backend-profile-design.md` §4. Each row is a
> REAL run against a real, host-authenticated harness through the REAL daemon
> path (`AgentLoop::run_turn_for`/`AgentLoop`'s task-delegation surface) —
> not adapter-level conformance (that's A-03/A-04/A-05).

## A-06 — runtime-backed primary conversation (mode 2)

**Status: PASS, run live.**

| Field | Value |
|---|---|
| Test | `tests/agent_runtime_backend_live.rs::a06_runtime_backed_conversation_live` |
| Command | `cargo test --test agent_runtime_backend_live -- --ignored --nocapture` |
| Harness | `AcpxAgentRuntime("claude")` → `acpx 0.12.0` → Claude Code (host session auth) |
| Path exercised | `AgentLoop::run_turn_for` → `run_turn_for_with_trust` → `ConversationBackend::Runtime("acpx_claude")` branch → `AgentLoop::run_runtime_backed_turn` (real `main.rs`/`AgentLoop` composition path, not a bypass) |
| Prompt | `"Reply with exactly this and nothing else: BASTION-A06-OK"` |
| Result | `health().ready == true` (acpx 0.12.0 detected); response == `"BASTION-A06-OK"`; assistant response found in session history (`SessionManager::load_recent`) — i.e. Bastion's own memory/conversation record, not just the harness's own session |
| Run at | 2026-07-14, this cycle |
| Cost/parsimony | one tiny prompt, no tool calls, ~7s wall time |

What this proves, concretely: a turn that starts at `AgentLoop::run_turn_for` (the exact same entry point every channel funnels through), with `BackendProfile.conversation == Runtime("acpx_claude")`, is served ENTIRELY by the external harness's tool-loop (Claude Code via acpx), and the response comes back through Bastion's normal turn-completion path: persisted to the SQLite session store, returned as the turn's answer. Codex was not used for A-06 (acpx→Claude Code was cheaper/already-authenticated on this host and sufficient to prove the mode-2 wiring; A-03/A-04 already validated both adapters individually at the conformance layer).

### Known scope limits of the mode-2 integration (this cycle, not a defect)

- **Permission requests are NOT resolved cross-turn yet.** The daemon serializes through one `&mut agent` (AGENTS.md architecture law) — `run_runtime_backed_turn` runs synchronously inside one turn and cannot block waiting for a LATER turn's plain-language "sim"/"não" reply. A `PermissionRequest` event this cycle is audited into the same `ApprovalGate` (Ciclo 2.1) and then answered `Deny { scope: DenyScope::Turn }` unconditionally — fail-closed, consistent with the Model path's own Turn-scoped-denial semantics, but NOT yet the rich "owner approves mid-task" UX the design doc explicitly scopes to M4-pleno (§6). A-06's prompt was deliberately tool-call-free so this path was never exercised by the passing run above.
- **No trace-context handoff to the harness.** `OtelContext` is left at its default; the existing `invoke_agent` root span still wraps the whole runtime-backed call (process-level correlation), but neither shipped adapter's protocol (codex app-server JSON-RPC, acpx NDJSON) has a slot to carry a `trace_id`/`parent_span_id` into the harness process itself.
- **Workspace root is a fixed per-owner temp directory** (`runtime_workspace_root`, `<tmp>/bastion-agent-runtime-workspaces/<owner>`), not yet a configurable per-deployment policy.

## A-07 — delegated task (mode 3)

**Status: PASS, run live.**

| Field | Value |
|---|---|
| Test | `tests/agent_runtime_delegated_task_live.rs::a07_delegated_task_concurrent_cancel_and_resume_live` |
| Command | `cargo test --test agent_runtime_delegated_task_live -- --ignored --nocapture` |
| Harness | `CodexAppServerRuntime` → `codex-cli 0.144.1` (host ChatGPT login) |
| Surface exercised | `AgentLoop::delegate_task` / `AgentLoop::cancel_delegated_task` / `AgentLoop::resume_delegated_task` — the real host-level API (not a bypass); conversation backend stayed `Model` throughout (delegation is independent of the conversation backend) |
| Run at | 2026-07-14, this cycle | 16.14s total wall time |

Four things proven in one run, all through the real methods:

1. **Delegation is non-blocking.** `delegate_task` (task1, prompt `"Reply with exactly this and nothing else: BASTION-A07-TASK1-OK"`) returned in 1.64s (start + submit only — it does not wait for the task).
2. **The conversation stays responsive concurrently.** Immediately after delegating task1, a normal `run_turn_for` call (Model backend, mocked provider) on the SAME `AgentLoop` completed in 19ms — not blocked on the background task.
3. **Cancel works and reports back.** Task2 (a `sleep 15 && echo done` shell prompt) was cancelled ~2s after delegation via `cancel_delegated_task`; the harness reported `TaskOutcome::Cancelled` ~2s later, delivered via the `pending_tx` PROACT-05 seam as `"[Tarefa delegada '...' cancelada]"`.
4. **Resume-after-restart works, with a disclosed contract limitation.** A third session was started directly, warmed up with one completed turn (codex only persists a resumable rollout after a real turn ran — same finding `codex_v2_resume_smoke` documented), then the process was killed (`drop(session)`, `kill_on_drop`) to simulate a daemon restart. `AgentLoop::resume_delegated_task` reattached the session successfully and submitted a follow-up task (`"...BASTION-A07-RESUME-OK"`), which completed and delivered its result via the same `pending_tx` path. The adapter correctly surfaced a `Warning{code: DegradedTransport}` on resume — codex's `thread/resume` protocol has no field for `PermissionProfile`, so the reattached thread kept its original `approvalPolicy` (documented in `codex.rs`, re-confirmed live here).

### Known scope limits / findings from this cycle (not defects — see rationale in code + `run_runtime_backed_turn`'s rustdoc, shared by mode 3)

- **No cross-restart task continuation in the contract.** `AgentRuntime::resume` reattaches the harness SESSION; neither shipped adapter buffers/replays events for a task that was already in flight when the connection was lost. `resume_delegated_task` is honest about this: it submits a NEW follow-up task on the reattached session rather than pretending to continue the original one. A richer "the exact same task keeps going across a restart" guarantee is not something this contract can deliver today without a protocol-level replay mechanism neither codex nor acpx expose.
- **Permission requests during a delegated task get the same fail-closed audited-deny as mode 2** (`Deny { scope: DenyScope::Turn }`) — a task that genuinely needs a tool call approved will end up cancelled by its own adapter (Codex's `respond_permission` cancels gracefully on a `Turn`-scoped deny). Neither task1 nor task3 needed a tool call in this run (pure conversational replies); task2's shell command was cancelled by the EXPLICIT `cancel_delegated_task` call before any approval negotiation was observed in the trace.
- **`pending_tx`/`pending_rx` is not owner-routed.** The PROACT-05 seam this reuses was built for the single-owner CLI daemon's goal-drift nudges (`main.rs`'s `pending_rx` arm feeds `agent.run_turn(&msg)` — always `DEFAULT_OWNER`). Reusing it for a multi-owner deployment's delegated-task notifications is a real gap (a result meant for owner B could surface as a proactive turn for the default owner) — richer, owner-scoped delivery is M4-pleno scope, not addressed here.
- **acpx cannot be used for the resume leg** (`supports.resume = false`, always `NotResumable` — by honest design, see `acpx.rs`) — A-07's resume proof required Codex specifically; acpx remains a valid `task_runtime` choice for the non-resume parts (delegate/cancel).
