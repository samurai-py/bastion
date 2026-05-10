# Bastion Strategy

> Living document. Read by GSD as grounding for all phases. Update when decisions change.

---

## Target Problem

People who want a private, self-hosted AI agent face a painful tradeoff: either use a cloud product (no privacy, subscription lock-in) or build their own (requires significant infra work). Current self-hosted runtimes are either too simple (no memory, no proactive behavior) or too complex (enterprise overhead, broken permissions in containers, Python dependency hell).

Bastion v3 eliminates that tradeoff.

---

## Approach

Replace the OpenClaw (Node.js) runtime with a **Rust-first agent core** inspired by Nanobot's clean architecture and ZeroClaw's trait system. The result: a single ~15MB binary, sub-50ms startup, `FROM scratch` Docker images, and zero permission issues in containers.

Skills remain Python where it makes sense (ONNX embeddings, LLM orchestration) — they run as isolated MCP servers. The Rust core never knows Python exists; it only speaks MCP.

The skill format (SKILL.md) stays compatible with the existing Nanobot/OpenClaw/agentskills.io ecosystem. Existing Bastion skills work without modification.

---

## Key Decisions

### What we're building
- **Rust runtime**: tokio async, trait-driven (Provider, Channel, Tool, Memory, Hook, Observer)
- **AgentLoop**: Nanobot (HKUDS) pattern — MessageBus → context build → LLM call → tool execution → session save
- **Nanobot patterns included**: AutoCompact (session TTL), Consolidator (token-based context compression), Dream (idle memory distillation), pending queue (mid-turn injection), provider hot-swap, strip_think (removes `<think>` blocks), command router (`/stop`, `/model`, etc.)
- **NanoClaw (qwibitai) philosophy adopted**: OS-level container isolation (not application-level), agents never hold raw API keys, codebase small enough to audit, channels installed on-demand via skills — not built-in
- **Built-in channels**: Telegram + Webhook. Channel trait for community extensions via `/add-<channel>` skills.
- **Providers**: Anthropic, OpenAI, Ollama (OpenAI-compatible) — full local LLM support out of the box
- **MCP client**: Composio as primary API gateway + any MCP server
- **Container**: `FROM scratch` Docker. Core binary + SKILL.md files only. Python MCP servers in separate containers with their own permissions.

### Skill layers (4 types)
| Layer | Examples | Language |
|-------|----------|----------|
| Rust built-in (trait impl) | proactive-engine, guardrails, output-validator, persona-engine, weight-system, life-log, mobile-connect | Rust |
| Python MCP server | memupalace (ONNX embeddings), skill-writer, self-improving | Python |
| SKILL.md pure markdown | weekly-review, crisis-mode, onboarding | Markdown |
| Composio MCP | bastion-calendar, any external API | Composio |

### memupalace = memU + mempalace (resolved)
Our memupalace is a deliberate merge of two external projects:
- **From memU (NevaMind-AI)**: proactive always-on memory, token cost reduction via cached insights (avoids redundant LLM calls), user intent capture, knowledge graph with hierarchical structure (categories → items → cross-references)
- **From mempalace (mempalace/mempalace)**: wing/room taxonomy, MCP server interface, semantic search, **query sanitizer** (critical: when LLM prepends system prompt to search queries, retrieval drops 89.8% → 1.0%; sanitizer recovers to 70-89%)
- mempalace has shipped new features since we built our initial version — audit and merge relevant ones before Phase 3.

### Token cost optimization (first-class concern)
Not an afterthought — a core design pillar:
- **AutoCompact + Consolidator**: compress session before hitting context limits, avoiding expensive context rebuilds
- **Progressive tool loading**: tool names in context by default, schemas loaded on demand (not all upfront)
- **memU cache layer**: cache distilled insights; always-on proactive agent never reruns full LLM calls for known facts
- **Local LLM via Ollama**: zero cloud cost for users who self-host models
- **Daily budget cap**: configurable hard cap on cloud API spend (like `DAILY_BUDGET_USD` in Aiden)
- **Dream idle distillation**: extract durable facts during idle time; reduces tokens needed in future sessions

### Installation philosophy
One command. Claude handles failures automatically (NanoClaw's model):
```bash
bash <(curl -fsSL https://bastion.run/install)
```
- Installer detects missing deps (Docker, API keys) and resolves them
- If a step fails, Claude Code is invoked to diagnose and resume — not an error wall
- No config sprawl: one `bastion.toml` with sensible defaults
- User provides: infrastructure (Docker) + API keys. Nothing else required.
- Goal: under 5 minutes from zero to working agent on Telegram

### What we're NOT building (from ZeroClaw — discarded)
- 21 messaging channels built-in (we ship 2, community adds more via skills)
- robot-kit / hardware peripherals (STM32, RPi GPIO)
- Enterprise security model (pairing ceremony, multi-level estop, policy engine)
- Full BMAD/enterprise planning overhead

### Architecture must not block
- **Bastion Cloud**: managed deployment for non-technical users. Single binary + Docker-first architecture already enables this. Keep the path open.
- **APK/IPA**: Telegram covers the mobile use case for v3. A native app is Phase 5+ and optional. Webhook channel is the integration point when the time comes.

---

## Personas / Users

**Primary**: Solo technical user who values privacy, runs their own infra, wants an agent that actually remembers and acts proactively — without babysitting containers or fighting pip permissions.

**Secondary**: Small team or family using a shared self-hosted instance via Telegram.

**Not targeting**: Enterprise teams, non-technical users (Bastion Cloud is the answer for those).

---

## Success Metrics

- Binary size: ≤ 20MB
- Cold start: ≤ 100ms
- Docker image: ≤ 50MB (core only)
- Installation: `docker compose up` after filling `.env` — under 5 minutes
- Container permissions: zero manual permission fixes required
- Skill compatibility: all existing Bastion SKILL.md skills load without modification
- Local LLM: full offline operation via Ollama (zero cloud dependency)

---

## Phases

> Phases are sequential implementation blocks, not milestones. Phase 1 is a complete rewrite — done as a single unit.

### Phase 1 — Core Rust Runtime *(big rewrite, done at once)*
Replace OpenClaw entirely. Deliver: AgentLoop, MessageBus, ToolRegistry, SkillsLoader, SessionManager (sqlite), Provider trait (Anthropic + OpenAI + Ollama), MCP client, AutoCompact, Consolidator, Dream, pending queue, provider hot-swap, strip_think, command router. CLI: `bastion agent -m "..."` and `bastion daemon`.

### Phase 2 — Built-in Rust Skills
Implement Rust trait impls for: Channel (Telegram + Webhook), Hook (guardrails + output-validator), CronService (proactive-engine + heartbeat), Memory trait (sqlite backend + Dream integration), Observer (life-log).

### Phase 3 — Python MCP Skill Servers
Port to isolated MCP servers: memupalace (ONNX embeddings), skill-writer, self-improving. Each runs as its own Docker container.

### Phase 4 — Deploy & Packaging
`FROM scratch` Dockerfile for core. docker-compose with MCP servers isolated. `bastion.toml` with sensible defaults. One-line installer (`bash <(curl -fsSL https://bastion.run/install)`). SKILL.md compatibility verified against agentskills.io format.

### Phase 5 — Ecosystem & Mobile *(optional, post-v3)*
Flutter companion app (webhook + SSE). agentskills.io publishing. ClawHub migration path. Bastion Cloud groundwork.

---

## Non-Goals

- Voice (STT/TTS) — not in scope for v3
- Computer use / screen automation — not in scope
- Web UI / dashboard — Telegram is the UI
- Multi-tenant SaaS — Bastion Cloud is a separate product
- Supporting every LLM provider — Anthropic + OpenAI-compat covers everything including Ollama, Groq, OpenRouter
