# Roadmap: Bastion v3

**Created:** 2026-05-10
**Granularity:** Coarse (4 sequential phases + optional Phase 5)
**Mode:** Standard (Horizontal Layers — Phase 1 é big rewrite atômico)
**Source of truth:** STRATEGY.md (raiz) + .planning/PROJECT.md + .planning/REQUIREMENTS.md

---

## Visão Geral

| # | Phase | Goal | Requirements | Plans (estimado) |
|---|-------|------|--------------|------------------|
| 1 | Core Rust Runtime | Substituir OpenClaw inteiro: AgentLoop, providers, MCP client, CLI funcional | CORE-01..12, PROV-01..06, MCP-01..02 (20 reqs) | 1-3 |
| 2 | Built-in Rust Skills | Channels, hooks, memory, persona, proactive — tudo em Rust | CHAN-01..04, PERS-01..07, MEM-01..05, HOOK-01..05, PROACT-01..05 (26 reqs) | 2-4 |
| 3 | Python MCP Skill Servers | memupalace, skill-writer, self-improving isolados em containers | MCP-03, MUPL-01..07, SKWR-01..05, SELF-01..02 (15 reqs) | 2-3 |
| 4 | Deploy & Packaging | FROM scratch Docker, installer one-line, cutover v2 → v3 | PKG-01..09 (9 reqs) | 1-2 |
| 5 | Ecosystem & Mobile *(opcional, pós-v3)* | Flutter app, agentskills.io, Bastion Cloud groundwork | MOB-01..02, ECO-01..03, CHEX-01..03 (v2) | — |

**Cobertura v1:** 69/69 requisitos mapeados ✓

---

## Phase 1: Core Rust Runtime

**Goal:** Substituir OpenClaw inteiro por um core Rust funcional. CLI `bastion agent -m "..."` e `bastion daemon` operando com Anthropic + OpenAI + Ollama. MCP client conectando a Composio. Sessões persistidas em sqlite. AgentLoop completo (Nanobot pattern). Phase 1 é big rewrite atômico — não há valor incremental dentro dela.

**Requirements:**
CORE-01, CORE-02, CORE-03, CORE-04, CORE-05, CORE-06, CORE-07, CORE-08, CORE-09, CORE-10, CORE-11, CORE-12, PROV-01, PROV-02, PROV-03, PROV-04, PROV-05, PROV-06, MCP-01, MCP-02

**Success Criteria:**
1. `bastion agent -m "olá"` retorna resposta válida usando Anthropic, OpenAI e Ollama (sem mudar binário)
2. `bastion daemon` mantém sessão sqlite persistente; restart não perde contexto
3. AutoCompact dispara antes de atingir limite de contexto e mantém conversa coerente
4. Comando `/model claude-opus-4-7` troca provider em runtime sem restart
5. MCP client conecta a Composio e expõe pelo menos uma tool externa funcional

**Anti-goals:**
- Nenhum canal externo (Telegram/Webhook) ainda — só CLI
- Nenhuma persona ainda — só agent default
- Nenhuma memória avançada — só sessão sqlite
- Nenhum Docker ainda — rodar via cargo run / binário local

---

## Phase 2: Built-in Rust Skills

**Goal:** Adicionar todas as skills críticas que precisam ser Rust (performance, isolamento ou trait-driven): canais (Telegram + Webhook), hooks (guardrails + output-validator), persona system completo (incluindo roteador LLM e múltiplas personas paralelas), memória sqlite, proactive engine (heartbeat + evento + idle).

**Requirements:**
CHAN-01, CHAN-02, CHAN-03, CHAN-04, PERS-01, PERS-02, PERS-03, PERS-04, PERS-05, PERS-06, PERS-07, MEM-01, MEM-02, MEM-03, MEM-04, MEM-05, HOOK-01, HOOK-02, HOOK-03, HOOK-04, HOOK-05, PROACT-01, PROACT-02, PROACT-03, PROACT-04, PROACT-05

**Success Criteria:**
1. Mensagem chega via Telegram, roteador LLM seleciona persona correta entre as 8 disponíveis e responde
2. Comando `/as carreira <pergunta>` força persona específica
3. Solicitação que envolve duas áreas (ex: "estou ansioso com prazo de trabalho e atrasando treino") aciona personas em paralelo
4. Heartbeat agendado dispara mensagem proativa em horário fixo via Telegram
5. Trigger de evento (calendar webhook) dispara comportamento proativo sem usuário pedir
6. Idle inferencístico (Dream) extrai fato e o usa em conversa futura sem pergunta explícita
7. Guardrails bloqueiam input malicioso antes de chegar ao LLM; output-validator filtra saída
8. Webhook recebe POST e responde com resposta do agente em JSON

**Anti-goals:**
- memupalace ainda não — memória usa só sqlite + tags
- skill-writer ainda não — skills criadas manualmente
- Sem Docker ainda — desenvolvimento local

---

## Phase 3: Python MCP Skill Servers

**Goal:** Os três skills Python (memupalace, skill-writer, self-improving) rodando como MCP servers isolados, cada um em seu container Docker com permissões próprias. memupalace funcionando como camada de memória rica (cache, knowledge graph, query sanitizer). skill-writer permite criar/editar skills só por conversa.

**Requirements:**
MCP-03, MUPL-01, MUPL-02, MUPL-03, MUPL-04, MUPL-05, MUPL-06, MUPL-07, SKWR-01, SKWR-02, SKWR-03, SKWR-04, SKWR-05, SELF-01, SELF-02

**Success Criteria:**
1. memupalace MCP server roda em container isolado, embeddings ONNX local, sem acesso a API keys do core
2. Query sanitizer demonstrado: query com system prompt prepended retorna ≥70% recall (vs ~1% sem sanitizer)
3. Mario diz "toda sexta às 18h me lembra de revisar metas semanais" no Telegram → skill-writer cria SKILL.md, instala, ativa, e na sexta seguinte a skill dispara
4. Mario diz "no lembrete de metas, não me cobra antes das 9h" → skill-writer edita skill existente; comportamento ajusta
5. Audit do upstream mempalace executado e features novas incorporadas (registrado em `.planning/decisions/mempalace-audit.md`)
6. self-improving promote loop sugere melhoria com base em uso real

**Anti-goals:**
- Sem installer one-line ainda — devs rodam via docker-compose
- Sem cutover de v2 ainda

---

## Phase 4: Deploy & Packaging — Cutover v2 → v3

**Goal:** Empacotar tudo em distribuição production-ready: core em Docker `FROM scratch`, MCP servers em containers separados, `bastion.toml` com defaults sensatos, installer one-line resolvendo dependências automaticamente. Phase 4 é o ponto de cutover oficial — Mario para de usar v2 e adota v3 como agente pessoal diário.

**Requirements:**
PKG-01, PKG-02, PKG-03, PKG-04, PKG-05, PKG-06, PKG-07, PKG-08, PKG-09

**Success Criteria:**
1. `docker build` produz imagem core ≤ 50MB; binário ≤ 20MB; `time docker run` mostra cold start ≤ 100ms
2. `docker compose up` (após `.env` preenchido com Anthropic/OpenAI/Telegram tokens) dispara core + memupalace + skill-writer + self-improving e Mario manda primeira mensagem ao bot Telegram em ≤ 5 minutos
3. Zero `chmod`/`chown` manual em volumes; permissões resolvidas via Dockerfile + compose
4. `bash <(curl -fsSL https://bastion.run/install)` em máquina limpa instala Docker se faltar, configura `.env`, sobe stack — falhas escalam para Claude Code que diagnostica e retoma
5. Suite de skills migradas (8 personas reescritas + skills Rust + 3 MCP servers Python) valida em testes de integração
6. SKILL.md de pelo menos uma skill externa do agentskills.io carrega sem modificação
7. Mario declara cutover formal: v2 desligada, v3 é o sistema pessoal por ≥ 7 dias contínuos sem rollback

**Anti-goals:**
- Sem Flutter app
- Sem agentskills.io publishing
- Sem Bastion Cloud

---

## Phase 5: Ecosystem & Mobile *(opcional, pós-v3)*

**Goal:** Estender Bastion para fora do uso pessoal de Mario — companion app mobile, publicação de skills no agentskills.io, groundwork para Bastion Cloud.

**Requirements:** v2 (MOB-01, MOB-02, ECO-01, ECO-02, ECO-03, CHEX-01, CHEX-02, CHEX-03)

**Success Criteria:**
1. Flutter companion app lê webhook + SSE, exibe interações em tempo real
2. Pelo menos uma skill custom de Bastion publicada e instalada via agentskills.io
3. ClawHub migration path documentado e validado com pelo menos um skill migrado
4. Bastion Cloud arquitetura inicial documentada (não necessariamente implementada)

**Anti-goals:**
- Phase 5 só inicia se Phases 1-4 estabilizarem em produção pessoal por ≥ 30 dias

---

## Dependency Graph

```
Phase 1 (Core Rust)
   ↓ requires
Phase 2 (Built-in Skills) — depends on AgentLoop, Provider trait, MCP client
   ↓ requires
Phase 3 (MCP Skill Servers) — depends on MCP client + Memory trait + Persona system
   ↓ requires
Phase 4 (Deploy) — packages everything from 1-3
   ↓ optional
Phase 5 (Ecosystem) — gated on production stability
```

Sequencial, sem paralelismo entre fases. Dentro de cada fase, plans podem rodar em paralelo (config.parallelization=true).

---

## Notas Cross-Phase

- **STRATEGY.md** é fonte primária para decisões arquiteturais. Atualizar STRATEGY.md ao tomar nova decisão estrutural; ROADMAP reflete escopo, não decisões.
- **memupalace audit** (MUPL-07) é gating item — feito no início de Phase 3 antes de iniciar implementação.
- **Cutover v2 → v3** ocorre no fim de Phase 4. Antes disso, v2 (OpenClaw) continua em produção pessoal.
- **Personas/skills v2 podem ser reescritas** se v3 simplificar — não tratar v2 como contrato vinculante.

---
*Created: 2026-05-10*
*Last updated: 2026-05-10 after initialization*
