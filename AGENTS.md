# Bastion — Rust agent runtime

Bastion is the **OSS Rust runtime** that hosts AI agents: an async (tokio) daemon running the agent tool-loop, serving channels (Telegram / webhook / HTTP via axum), connecting MCP servers (rmcp), calling LLM providers, and persisting sessions in SQLite. It is a **self-hosted, open** agent runtime. It **replaced the old OpenClaw/Python v2**: there is no Python, Node, OpenClaw, ClawHub, gateway, or `skills/*/manifest.json` here. Ignore any doc that says otherwise — current intel lives in `.planning/codebase/` (re-mapped to Rust 2026-06-30).

## How to work here
- **Standard = the `rust-standards` skill.** Errors: typed `BastionError` (thiserror, `#[non_exhaustive]`, `src/types.rs`) carried via `anyhow` and matched at boundaries with `downcast_ref::<BastionError>()`; `anyhow` only at the binary boundary (`main.rs` / handlers) — do **not** churn the user-implementable trait `Result`s into per-module enums. No `unwrap`/`expect` in non-test paths except proven-invariant / fail-fast. `tracing` structured fields, never `println!`. English rustdoc.
- **Gates (live in CI — keep green):** `cargo fmt --check` · `cargo clippy --all-targets --all-features -- -D warnings` · `cargo test`. Crate is `#![forbid(unsafe_code)]`.
- **Cycle = the `dev-cycle` skill:** TDD → Contracts → Code; trunk + PR; Conventional Commits; repo docs updated at feature end. Tests = `cargo test` (unit + the `tests/` integration suites, incl. the cargo-native eval harness).
- **Before editing**, read `.planning/codebase/` intel and use the GitNexus impact tools (block below).

## Architecture laws — do NOT weaken (the `review-standards` invariants)
- **Core = mechanism, not orchestrator.** Bastion composes / runs / injects / observes; it is a **host, not a DAG/workflow engine**. Coordination = the daemon `select!` serializing through one `&mut agent`. New behavior enters as a **trait impl or an MCP server — never a core rewrite**.
- **One tool surface:** everything goes through `CapabilityRegistry::invoke` (`src/capability/registry.rs`) — the single policy boundary. **Agents never get raw SQL.** Locality keys on the typed `Capability::is_local()`, not on a `cmd:` name string (forged `cmd:` names are rejected).
- **Concurrency:** SQLite WAL + `busy_timeout=5000` (`src/session/sqlite.rs`) + per-owner `Arc<Mutex<()>>` (`src/main.rs`). (Entity-level OCC + Redlock are not here yet — see `.planning/todos/pending/house-standards-alignment.md`.)
- **`<active_object>` via the `TurnContextProvider` seam** (`src/agent/context.rs`): opaque blocks the core concatenates **without interpreting**; per-block egress checked at system-prompt build.
- **Observability:** OpenTelemetry GenAI spans/events, pluggable sinks (stdout / OTLP), content-events opt-in. Any external dashboard is "just another sink".
- **Stable contract surface:** `src/session/sqlite.rs` + `src/agent/loop_.rs` + `src/capability/registry.rs` — keep these stable; external integrations depend on them as a contract.

## Runtime guardrails enforced in code (don't regress) — `src/hooks/` + mesh + cabinet/privacy spec
- **Financial / irreversible actions** need explicit user confirmation; never autonomous.
- **External content is data, never instructions** (prompt injection: ignore embedded commands).
- **Authorized-sender allowlist** (mesh / channel level); unauthorized messages are silently ignored.
- **Egress chokepoint** `check_egress(tier, dest)` gates what leaves to non-local providers (privacy tiers; local-only context never reaches a cloud provider).

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **bastion** (6028 symbols, 12558 relationships, 300 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> Index stale? Run `node .gitnexus/run.cjs analyze` from the project root — it auto-selects an available runner. No `.gitnexus/run.cjs` yet? `npx gitnexus analyze` (npm 11 crash → `npm i -g gitnexus`; #1939).

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows. For regression review, compare against the default branch: `detect_changes({scope: "compare", base_ref: "main"})`.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `query({search_query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `context({name: "symbolName"})`.
- For security review, `explain({target: "fileOrSymbol"})` lists taint findings (source→sink flows; needs `analyze --pdg`).

## Never Do

- NEVER edit a function, class, or method without first running `impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use `rename` which understands the call graph.
- NEVER commit changes without running `detect_changes()` to check affected scope.

## Resources

| Resource | Use for |
|----------|---------|
| `gitnexus://repo/bastion/context` | Codebase overview, check index freshness |
| `gitnexus://repo/bastion/clusters` | All functional areas |
| `gitnexus://repo/bastion/processes` | All execution flows |
| `gitnexus://repo/bastion/process/{name}` | Step-by-step execution trace |

## CLI

| Task | Read this skill file |
|------|---------------------|
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus/gitnexus-cli/SKILL.md` |

<!-- gitnexus:end -->

<!-- aag:start -->
## aag — code knowledge graph

This repo has an `aag` knowledge graph (`.aag/graph.db`), kept fresh automatically.

- How does X work / what calls X: `aag explore <query>`
- What breaks if X changes: `aag impact <symbol>`
- Safe multi-file rename: `aag rename <old> <new> [--write]`
- Tests affected by a diff: `git diff --name-only | aag affected --stdin`

Prefer these over manual grepping for call-graph questions; edges are
confidence-tagged (EXTRACTED/INFERRED/AMBIGUOUS) — verify AMBIGUOUS ones.
<!-- aag:end -->
