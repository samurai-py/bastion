# State: Bastion v3

## Project Reference

See: `.planning/PROJECT.md` (updated 2026-05-10)

**Core value:** Bastion ajuda Mario a fazer suas metas anuais avançarem — proativo, customizável por linguagem natural, seguro e instalável em minutos.
**Current focus:** Phase 1 — Core Rust Runtime

## Current Phase

**Phase 1: Core Rust Runtime** — pending discussion/planning

Goal: Substituir OpenClaw inteiro por core Rust (AgentLoop, providers Anthropic/OpenAI/Ollama, MCP client, sessões sqlite, CLI funcional).

Next step: `/gsd-discuss-phase 1` para gather context, ou `/gsd-plan-phase 1` para pular discussão.

## Active Workstream

(none — fresh init)

## Recent Decisions

| Date | Decision | Source |
|------|----------|--------|
| 2026-05-10 | Roteamento de personas via classificação LLM + memória global tageada | Questioning |
| 2026-05-10 | Proatividade em 3 modos (heartbeat + evento + idle), sem intervenção mid-conversation | Questioning |
| 2026-05-10 | skill-writer fica em Phase 3 (depende de memU para padrões) | Questioning |
| 2026-05-10 | Cutover v2 → v3 na Phase 4 (após Docker scratch + installer) | Questioning |
| 2026-05-10 | Personas/skills v2 podem ser reescritas em v3 (compat total não é requisito) | Questioning |
| 2026-05-10 | Source-available com licença restritiva (estilo BSL/Polyform Strict) | Questioning |

## Files

| Artifact | Path |
|----------|------|
| Strategy | `STRATEGY.md` (raiz) |
| Project | `.planning/PROJECT.md` |
| Config | `.planning/config.json` |
| Requirements | `.planning/REQUIREMENTS.md` |
| Roadmap | `.planning/ROADMAP.md` |
| State | `.planning/STATE.md` |
| Codebase map | `.planning/codebase/` |

---
*Last updated: 2026-05-10 after initialization*
