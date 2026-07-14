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

**Status: <!-- filled in by Commit 3 -->**

(placeholder — populated once Commit 3 lands the delegation mechanism and its live test)
