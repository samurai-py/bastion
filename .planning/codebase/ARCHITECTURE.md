---
title: Architecture
last_mapped: 2026-05-10
---

# Architecture

## Pattern

**Hexagonal Architecture (Ports & Adapters)** applied per-skill.

Each skill defines:
- Domain models (dataclasses)
- Persistence protocols (Python `Protocol` classes — the ports)
- Adapters (SQLite, Supabase, file-system implementations)
- Pure logic functions that depend only on protocols, not concrete adapters

OpenClaw acts as the AI agent runtime. Skills are Python modules loaded by OpenClaw and invoked via skill manifests.

## Layers

```
┌─────────────────────────────────────────────┐
│  OpenClaw Runtime (Node.js)                 │
│  - Routes messages to personas              │
│  - Invokes skill handlers                   │
│  - Manages context and session              │
└─────────┬───────────────────────────────────┘
          │ skill invocation
┌─────────▼───────────────────────────────────┐
│  Skills Layer (Python 3.12)                 │
│  - persona-engine: persona matching/routing │
│  - weight-system: dynamic priority weights  │
│  - life-log: RAG memory storage/retrieval   │
│  - guardrails: safety rule enforcement      │
│  - crisis-mode: emergency replanning        │
│  - memupalace: semantic long-term memory    │
│  - mobile-connect: JWT-authenticated API    │
│  - bastion-calendar: Composio calendar      │
│  - proactive-engine: scheduled triggers     │
│  - onboarding: initial setup flow           │
│  - self-improving: agent self-reflection    │
│  - skill-writer: creates new skills         │
│  - output-validator: response validation    │
│  - weekly-review: weekly synthesis          │
└─────────┬───────────────────────────────────┘
          │ adapter
┌─────────▼───────────────────────────────────┐
│  Persistence Layer                          │
│  - SQLite + sqlite-vec (default)            │
│  - Supabase (optional cloud)                │
│  - File system (personas/, SOUL.md, etc.)   │
└─────────────────────────────────────────────┘
```

## Key Flows

### Message → Persona Routing
1. User message arrives via messaging channel (Telegram, etc.)
2. OpenClaw authenticates session (TOTP)
3. `persona-engine` matches message to personas via keyword + semantic matching
4. `weight-system` applies dynamic weights to select active persona
5. Selected persona's context loaded, LLM generates response
6. `guardrails` checks response before delivery
7. `life-log` stores interaction for future RAG retrieval

### Crisis Mode
1. Message contains `/crise` or urgency keywords → `crisis-mode` triggers
2. Detect algorithm scores confidence (> 0.8 = crisis)
3. Sacrifice algorithm: boosts persona weight by +0.3 (max 1.0)
4. Frees ≥ 2h Deep Work by canceling/moving low-priority tasks
5. Records crisis event in `personas/{slug}/MEMORY.md`

### Memory (Memupalace)
1. Interactions stored via `life-log` adapter
2. `memupalace` provides semantic search via ChromaDB + ONNX embeddings
3. `migrate_lifelog.py` handles migration from life-log format
4. Knowledge graph for relationship tracking

## Abstractions

| Abstraction | Location | Purpose |
|-------------|----------|---------|
| `PersonaPersistenceProtocol` | `skills/persona-engine/persona_engine.py` | SOUL.md I/O |
| `WeightPersistenceProtocol` | `skills/weight-system/` | Weight storage |
| `LifeLogProtocol` | `skills/life-log/db/protocols.py` | Log storage |
| `GuardrailEngine` | `skills/guardrails/guardrails.py` | Safety rules |

## Entry Points

- `docker-compose.yml` — primary entry (starts openclaw + caddy)
- `skills/proactive-engine/main.py` — CLI for scheduled tasks / HEARTBEAT
- `Caddyfile` — HTTP routing entry
