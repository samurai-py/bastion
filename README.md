# 🏰 Bastion

> Self-hosted, privacy-first AI agent runtime and personal agent — written in Rust.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

## Current status

Bastion is a native Rust/Tokio daemon. The old OpenClaw/Python/Node v2 implementation is no longer present.

- **Branch:** v1.1 — Cognition, Connection & Hardening.
- **Implementation observed:** commit 297841f, reconciled 2026-07-13.
- **v1.0:** shipped in the repository history.
- **v1.1:** phases 7–11 are code-complete; phase 12 live UAT plus the accumulated code/security review still gate release.
- **Documentation authority:** .planning/PROJECT.md and .planning/REQUIREMENTS.md define intent; .planning/STATE.md records execution; this README summarizes observed code. Pending work lives in .planning/todos/pending/.

“Implemented” below means present and covered by repository tests. It does not imply live verification against every provider, channel account or audio device.

## What Bastion does

Bastion hosts a longitudinal personal agent with multiple personas, persistent and contestable memory, tools, channels, proactive behavior and bounded autonomous execution. It is a host and agent runtime, not a workflow/DAG engine.

One policy boundary, CapabilityRegistry::invoke, mediates every tool call. New behavior enters through traits, MCP servers, channels or extensions rather than rewrites of the agent loop.

## Implemented capabilities

- **Agent runtime:** serialized Tokio daemon, tool loop, typed errors, SQLite WAL sessions and per-owner isolation.
- **Providers:** Anthropic, OpenAI, Gemini, Groq, OpenRouter and Ollama, plus the current terminal-agent adapter; structured-output fallback and bounded provider fallback.
- **Personas:** SOUL profiles, LLM routing, parallel execution and bounded Cabinet deliberation.
- **Memory and learning:** contestable beliefs with provenance/privacy tiers, bi-temporal validity, Dream consolidation, Reflector procedural learning and contestable stigmergic autonomous mode.
- **Tools and interoperability:** capability registry, MCP client, Bastion as an MCP server, portable seams and inference gateway for sidecars.
- **Channels:** terminal, Telegram and webhook; WhatsApp, Discord, Slack, email and local voice are implemented in code but remain part of the phase-12 live-verification gate.
- **Proactivity:** goals, heartbeat/event/idle triggers, cron scheduling and background learning.
- **Mesh:** encrypted selective context/belief exchange with owner allowlists and fail-closed egress checks.
- **Security:** authorization allowlists, privacy-tier egress, approval queue for destructive actions, untrusted-content spotlighting, capability quarantine, OAuth state protection and agent identity/card.
- **Observability:** OpenTelemetry GenAI spans/events with stdout and OTLP sinks.
- **Companion surface:** webhook/SSE APIs and the current companion/PokeDev clients; the revamp’s unified embedded Agent UI is planned, not claimed as complete.

## Explicit non-features

- No OpenClaw, ClawHub, Node.js gateway or Python agent core.
- No central DAG/workflow orchestrator.
- No raw SQL tool path.
- No commercial cloud control plane inside the OSS core.
- No enterprise entity OCC, Redlock or business-object timeline; those belong to the enterprise host above the runtime.
- No claim that code-only channel/provider support has passed phase-12 live UAT.

## Build and run

Prerequisites: a Rust toolchain and configuration in bastion.toml/.env.

~~~bash
cargo build
cargo test --workspace
cargo run -- daemon
~~~

Repository quality gates:

~~~bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
~~~

See AGENTS.md and .planning/codebase/ before changing code.

## Architecture at a glance

~~~text
channels / CLI / companion
          │
      AgentHandle
          │
      AgentLoop
  ┌───────┼────────┐
personas  providers  hooks
          │
 CapabilityRegistry
   ┌──────┼──────────┐
  MCP   direct tools  extensions
          │
 SQLite memory/session + OTel
~~~

The stable integration surface is AgentLoop, CapabilityRegistry, TurnContextProvider, session storage and neutral OTel events.

## Core vs Agent (upcoming repository split)

This repository is being reorganized into two clearly separated layers, and an upcoming split will move them into two repositories (no date promised):

- **Bastion Core** — the reusable substrate: the family of `crates/bastion-*` crates (runtime, types, memory, cognition, personas, mesh, MCP, providers, agent-runtime, extension protocol). Core is a *host and mechanism* — it never contains product-specific policy, a consuming application's business objects, or any closed-source/commercial concept. A second consumer (`examples/embedded-host-slice`) proves the boundary holds without importing the Agent.
- **Bastion Agent** — the personal product built *on top of* Core: the daemon/app in `src/`, concrete channels, config, installer, Docker, mobile companion, skills and packs. The Agent depends on Core through Core's public API only; the dependency never points the other way.

Until the physical split, both live in this one workspace and the crate-dependency CI gate (`scripts/check-crate-deps.sh`) enforces the one-way boundary. See `docs/revamp/M6-PREP.md` for the exact Core/Agent mapping and the split plan.

## Roadmap

1. **Finish v1.1:** phase-12 live UAT, accumulated code/security review and release gates.
2. **Bastion Core revamp:** extract stable Rust crates and host protocols without reducing the flagship Agent.
3. **Bastion Agent:** preserve the personal product, embedded local/hosted UI, extensions/packs/loadouts and structured task runtimes.
4. **Cloud readiness:** daemon lifecycle, health, volumes, secret references, import/export and auth hooks; the commercial cloud control plane remains external.
5. **Community:** conformance-tested extensions and packs, clear permissions and a competitive feature cadence.

Detailed private planning and product boundaries are maintained outside this public README; public implementation claims must remain verifiable in this repository.

## Documentation

- .planning/PROJECT.md — product intent and validated capabilities
- .planning/REQUIREMENTS.md — current milestone requirements
- .planning/STATE.md — execution state and gates
- .planning/codebase/ — code-derived architecture
- docs/pt-br/README.md and docs/en/README.md — archived v2 documentation until rewritten for the Rust runtime

## License

MIT — see LICENSE.
