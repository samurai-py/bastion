---
title: External Integrations
last_mapped: 2026-05-10
---

# External Integrations

## LLM Providers (via `.env`)

| Provider | Config Key | Notes |
|----------|-----------|-------|
| OpenRouter | `LLM_PROVIDER=openrouter` | Recommended — access to many models including free |
| Anthropic | `LLM_PROVIDER=anthropic` | Direct API |
| OpenAI | `LLM_PROVIDER=openai` | Also used for Ollama local (`OPENAI_BASE_URL`) |
| Gemini | `LLM_PROVIDER=gemini` | Google |
| Groq | `LLM_PROVIDER=groq` | Fast inference |
| Ollama (local) | `OPENAI_BASE_URL=http://ollama:11434/v1` | Self-hosted, optional docker service |

## Composio

- **Purpose**: 850+ app integrations (Google Calendar, Notion, GitHub, etc.)
- **Config**: `COMPOSIO_CONSUMER_KEY` in `.env`
- **Used by**: `skills/bastion-calendar/` (primary consumer, tested via `test_composio_contract.py`)

## Database

| Option | Config | Notes |
|--------|--------|-------|
| SQLite | `DB_STRATEGY=sqlite`, `SQLITE_PATH=./db/life-log.db` | Default — no external service needed |
| Supabase | `DB_STRATEGY=supabase`, `SUPABASE_URL`, `SUPABASE_KEY` | Cloud alternative |

SQLite uses `sqlite-vec` extension for vector search (RAG/semantic memory).

## Messaging Channels (via `.env`)

| Channel | Config Keys | Notes |
|---------|------------|-------|
| Telegram | `TELEGRAM_BOT_TOKEN`, `TELEGRAM_CHAT_ID` | Primary messaging channel |
| Discord | `DISCORD_TOKEN`, `DISCORD_CHANNEL_ID` | Optional |
| Slack | `SLACK_BOT_TOKEN`, `SLACK_CHANNEL_ID` | Optional |
| WhatsApp | `EVOLUTION_API_URL`, `EVOLUTION_API_KEY` | Via Evolution API |

## Authentication

| Service | Config Keys | Used by |
|---------|------------|---------|
| TOTP (pyotp) | `TOTP_SECRET` (generated at onboarding) | All sessions |
| JWT (PyJWT) | `JWT_SECRET` | `skills/mobile-connect/` |

## Reverse Proxy

- **Caddy** — HTTPS termination, routing to OpenClaw
- Config: `Caddyfile` at project root
- Port 443/80 exposed by caddy container

## Ports (OpenClaw)

| Port | Binding | Purpose |
|------|---------|---------|
| 18789 | 127.0.0.1 | Main OpenClaw port |
| 18791–18794 | 127.0.0.1 | Additional OpenClaw services |
