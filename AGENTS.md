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

This project is indexed by GitNexus as **bastion** (4222 symbols, 10096 relationships, 300 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> If any GitNexus tool warns the index is stale, run `npx gitnexus analyze` in terminal first.

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `gitnexus_impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `gitnexus_detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `gitnexus_query({query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `gitnexus_context({name: "symbolName"})`.

## When Debugging

1. `gitnexus_query({query: "<error or symptom>"})` — find execution flows related to the issue
2. `gitnexus_context({name: "<suspect function>"})` — see all callers, callees, and process participation
3. `READ gitnexus://repo/bastion/process/{processName}` — trace the full execution flow step by step
4. For regressions: `gitnexus_detect_changes({scope: "compare", base_ref: "main"})` — see what your branch changed

## When Refactoring

- **Renaming**: MUST use `gitnexus_rename({symbol_name: "old", new_name: "new", dry_run: true})` first. Review the preview — graph edits are safe, text_search edits need manual review. Then run with `dry_run: false`.
- **Extracting/Splitting**: MUST run `gitnexus_context({name: "target"})` to see all incoming/outgoing refs, then `gitnexus_impact({target: "target", direction: "upstream"})` to find all external callers before moving code.
- After any refactor: run `gitnexus_detect_changes({scope: "all"})` to verify only expected files changed.

## Never Do

- NEVER edit a function, class, or method without first running `gitnexus_impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use `gitnexus_rename` which understands the call graph.
- NEVER commit changes without running `gitnexus_detect_changes()` to check affected scope.

## Tools Quick Reference

| Tool | When to use | Command |
|------|-------------|---------|
| `query` | Find code by concept | `gitnexus_query({query: "auth validation"})` |
| `context` | 360-degree view of one symbol | `gitnexus_context({name: "validateUser"})` |
| `impact` | Blast radius before editing | `gitnexus_impact({target: "X", direction: "upstream"})` |
| `detect_changes` | Pre-commit scope check | `gitnexus_detect_changes({scope: "staged"})` |
| `rename` | Safe multi-file rename | `gitnexus_rename({symbol_name: "old", new_name: "new", dry_run: true})` |
| `cypher` | Custom graph queries | `gitnexus_cypher({query: "MATCH ..."})` |

## Impact Risk Levels

| Depth | Meaning | Action |
|-------|---------|--------|
| d=1 | WILL BREAK — direct callers/importers | MUST update these |
| d=2 | LIKELY AFFECTED — indirect deps | Should test |
| d=3 | MAY NEED TESTING — transitive | Test if critical path |

## Resources

| Resource | Use for |
|----------|---------|
| `gitnexus://repo/bastion/context` | Codebase overview, check index freshness |
| `gitnexus://repo/bastion/clusters` | All functional areas |
| `gitnexus://repo/bastion/processes` | All execution flows |
| `gitnexus://repo/bastion/process/{name}` | Step-by-step execution trace |

## Self-Check Before Finishing

Before completing any code modification task, verify:
1. `gitnexus_impact` was run for all modified symbols
2. No HIGH/CRITICAL risk warnings were ignored
3. `gitnexus_detect_changes()` confirms changes match expected scope
4. All d=1 (WILL BREAK) dependents were updated

## Keeping the Index Fresh

After committing code changes, the GitNexus index becomes stale. Re-run analyze to update it:

```bash
npx gitnexus analyze
```

If the index previously included embeddings, preserve them by adding `--embeddings`:

```bash
npx gitnexus analyze --embeddings
```

To check whether embeddings exist, inspect `.gitnexus/meta.json` — the `stats.embeddings` field shows the count (0 means no embeddings). **Running analyze without `--embeddings` will delete any previously generated embeddings.**

> Claude Code users: A PostToolUse hook handles this automatically after `git commit` and `git merge`.

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
