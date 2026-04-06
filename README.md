# 🏰 Bastion

> Self-hosted, privacy-first AI agent. Your personal Life OS — running entirely on your own machine.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

**[Português](docs/pt-br/README.md)** · **[English](docs/en/README.md)**

---

Bastion is a self-hosted AI orchestrator built on [OpenClaw](https://openclaw.ai). It connects to the messaging apps you already use (Telegram, WhatsApp, Discord, Slack), routes conversations through the LLM provider of your choice, and organizes your life through **personas** — behavioral profiles for each area of your life.

Your data never leaves your machine. No subscriptions. No cloud lock-in.

## Quick Start

```bash
bash <(curl -fsSL https://bastion.run/install)
```

Takes 5 minutes. You'll need Docker and two API keys:
- **LLM** — OpenRouter, Anthropic, OpenAI, Gemini, or Groq (OpenRouter has free models)
- **[Composio](https://composio.dev)** — for external integrations (Google Calendar, Notion, GitHub, etc.)

## How It Works

Bastion uses **personas** — each one is a specialized agent for a different area of your life (work, health, business, studies). When you send a message, Bastion automatically detects which persona should respond based on context and keywords.

Each persona has its own memory, tone, and set of skills. They share a common life-log for cross-context recall.

## Key Features

- **Persona system** — separate agents per life area, with dynamic weight-based routing
- **Life-log** — semantic memory with vector search (RAG) across all interactions
- **Crisis mode** — emergency replanning algorithm that frees up Deep Work time
- **Mobile app** — self-hosted iOS/Android app with secure JWT pairing
- **Skill system** — extensible via bundled skills and the ClawHub marketplace
- **TOTP auth** — every session requires a 6-digit code from your authenticator app
- **Anti-injection** — all external content treated as data, never as instructions

## Stack

- **Runtime**: OpenClaw (Node.js) + Docker + Caddy
- **Skills**: Python 3 with Hypothesis property-based testing
- **Mobile plugin**: TypeScript + Express + fast-check
- **Memory**: SQLite with vector search (sqlite-vec)
- **Security**: Sage plugin, TOTP, JWT, user allowlist

## Documentation

- 🇧🇷 [Documentação em Português](docs/pt-br/README.md)
- 🇺🇸 [English Documentation](docs/en/README.md)
- 📐 [Architecture Blueprint](docs/BLUEPRINT.md)

## Roadmap

| Status | Milestone |
|--------|-----------|
| ✅ | Bastion v1 — initial release |
| ✅ | Bastion v2 — OpenClaw-based, mobile app, RAG, PBT |
| 🔜 | Self-hosted mobile app (APK/IPA distribution) |
| 🔜 | Token cost optimization and local LLM support |
| 🔜 | Container isolation ([NanoClaw](https://github.com/qwibitai/nanoclaw)-inspired sandboxing) |
| 🔜 | Installer improvements and self-hosted LLM automation |
| 🔮 | Bastion v3 — [ZeroClaw](https://github.com/openagen/zeroclaw) Rust core + [memU](https://github.com/NevaMind-AI/memUBot) memory system |
| 🔮 | Bastion Cloud — managed deployment for non-technical users |

## License

MIT — see [LICENSE](LICENSE).
