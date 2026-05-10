---
title: Directory Structure
last_mapped: 2026-05-10
---

# Directory Structure

## Root Layout

```
bastion/
├── skills/                  # All skill modules (Python)
├── personas/                # Persona definitions (SOUL.md files)
├── config/                  # OpenClaw runtime config (bind-mounted)
├── db/                      # SQLite databases (bind-mounted)
├── docs/                    # Documentation (pt-BR, en, BLUEPRINT.md)
├── extensions/              # OpenClaw extensions (bind-mounted from ~/.openclaw/extensions)
├── tests/                   # Top-level tests (test-installer.sh)
├── docker-compose.yml       # Service orchestration
├── Dockerfile               # Python deps image
├── Caddyfile                # Reverse proxy config
├── pyproject.toml           # Python project config
├── SOUL.md                  # Root agent identity
├── USER.md                  # User preferences (writable)
├── AGENTS.md                # Agent instructions
├── HEARTBEAT.md             # Proactive weekly task definitions
├── STRATEGY.md              # Project strategy (untracked)
└── .env.example             # Environment variable template
```

## Skills Directory (`skills/`)

Each skill is a self-contained Python package:

```
skills/
├── bastion-calendar/        # Composio calendar integration
│   ├── __init__.py
│   ├── manifest.json
│   ├── models.py
│   ├── parser.py
│   └── tests/
│       ├── conftest.py
│       ├── test_composio_contract.py
│       └── test_parser.py
├── crisis-mode/             # Emergency replanning
│   ├── crisis_mode.py
│   ├── i18n.py
│   ├── manifest.json
│   └── tests/
│       └── test_crisis_properties.py
├── guardrails/              # Safety enforcement
│   ├── guardrails.py
│   ├── i18n.py
│   ├── manifest.json
│   └── tests/
│       └── test_guardrail_properties.py
├── life-log/                # Interaction memory (RAG)
│   ├── __init__.py
│   ├── manifest.json
│   ├── factory.py
│   ├── i18n.py
│   ├── life_log_cli.py
│   ├── db/
│   │   ├── protocols.py
│   │   ├── sqlite_adapter.py
│   │   └── supabase_adapter.py
│   └── tests/
├── memupalace/              # Semantic long-term memory
│   ├── __init__.py
│   ├── embedder.py
│   ├── factory.py
│   ├── knowledge_graph.py
│   ├── mcp_server.py
│   ├── migrate_lifelog.py
│   ├── models.py
│   ├── scorer.py
│   └── store.py
├── mobile-connect/          # JWT-authenticated mobile API
│   ├── manifest.json
│   └── tests/
├── onboarding/              # Initial setup flow
│   ├── manifest.json
│   └── tests/
├── output-validator/        # Response validation
│   ├── manifest.json
│   └── tests/
├── persona-engine/          # Persona matching & routing
│   ├── manifest.json
│   ├── persona_engine.py
│   ├── i18n.py
│   ├── pyproject.toml
│   └── tests/
├── proactive-engine/        # Scheduled triggers (HEARTBEAT)
│   ├── manifest.json
│   ├── main.py              # CLI entrypoint
│   └── tests/
├── self-improving/          # Agent self-reflection
│   ├── manifest.json
│   └── tests/
├── skill-writer/            # Creates new skills
│   ├── manifest.json
│   └── tests/
├── weekly-review/           # Weekly synthesis
│   ├── manifest.json
│   └── tests/
└── weight-system/           # Dynamic persona weights
    ├── manifest.json
    └── tests/
```

## Key File Locations

| File | Purpose |
|------|---------|
| `skills/*/manifest.json` | Skill metadata, version, entry points |
| `skills/*/i18n.py` | Internationalization helpers (delegates to `utils/i18n`) |
| `skills/*/tests/conftest.py` | Pytest fixtures per skill |
| `skills/*/db/protocols.py` | Hexagonal port definitions |
| `skills/life-log/factory.py` | Adapter factory (sqlite vs supabase) |
| `SOUL.md` | Root agent system prompt / identity |
| `HEARTBEAT.md` | Proactive weekly task schedule |

## Naming Conventions

- Skill dirs: `kebab-case` (e.g., `crisis-mode`, `life-log`)
- Python files: `snake_case` (e.g., `crisis_mode.py`, `life_log_cli.py`)
- Test files: `test_*.py` (pytest discovery)
- Test dirs: `tests/` inside each skill
- Protocol classes: `*Protocol` suffix
- Adapter classes: `*Adapter` suffix (e.g., `SqliteAdapter`, `SupabaseAdapter`)
