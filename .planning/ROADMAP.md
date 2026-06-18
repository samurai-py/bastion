# Roadmap: Bastion v3

**Created:** 2026-05-10
**Granularity:** Coarse (4 sequential phases + optional Phase 5)
**Mode:** Standard (Horizontal Layers — Phase 1 é big rewrite atômico)
**Source of truth:** STRATEGY.md (raiz) + .planning/PROJECT.md + .planning/REQUIREMENTS.md

---

## Visão Geral

| # | Phase | Goal | Requirements | Status |
|---|-------|------|--------------|--------|
| 1 | Core Rust Runtime | Substituir OpenClaw inteiro: AgentLoop, providers, MCP client, CLI funcional | CORE-01..12, PROV-01..08, MCP-01..02 (22 reqs) | ✅ done |
| 2 | Built-in Rust Skills | Channels, hooks, memory, persona, **cabinet, privacy tiers, goal engine**, proactive — tudo em Rust | CHAN-01..04, PERS-01..07, CAB-01..05, PRIV-01..04, MEM-01..09, GOAL-01..04, HOOK-01..05, PROACT-01..05 (43 reqs) | ✅ done |
| 3 | Python MCP Skill Servers | memupalace, skill-writer (+ **loop fechado**), self-improving isolados em containers | MCP-03, MUPL-01..07, SKWR-01..06, SELF-01..02 (16 reqs) | ✅ done |
| 4 | Deploy & Packaging | FROM scratch Docker, installer one-line, cutover v2 → v3 | PKG-01..09 (9 reqs) | ✅ cutover-live (boot ok; camada cognitiva pendente → Phase 5) |
| 5 | **v1.0 Cognitive Completion + Fabric-Ready Seams** | 5/6 | In Progress|  |
| 6 | Ecosystem & Mobile *(opcional, pós-v1.0)* | 5/7 | In Progress|  |

**Cobertura v1:** 89/89 requisitos mapeados ✓ (69 originais + 18 de diferenciação vs Hermes + 2 providers OpenRouter/Gemini, 2026-05-30)

---

## Phase 1: Core Rust Runtime

**Goal:** Substituir OpenClaw inteiro por um core Rust funcional. CLI `bastion agent -m "..."` e `bastion daemon` operando com Anthropic + OpenAI + Ollama. MCP client conectando a Composio. Sessões persistidas em sqlite. AgentLoop completo (Nanobot pattern). Phase 1 é big rewrite atômico — não há valor incremental dentro dela.

**Requirements:**
CORE-01, CORE-02, CORE-03, CORE-04, CORE-05, CORE-06, CORE-07, CORE-08, CORE-09, CORE-10, CORE-11, CORE-12, PROV-01, PROV-02, PROV-03, PROV-04, PROV-05, PROV-06, PROV-07, PROV-08, MCP-01, MCP-02

**Plans:** 3 plans in 2 waves

Plans:
**Wave 1**
- [x] 01-01-PLAN.md — Provider layer: types, Provider trait, Anthropic SSE, OpenAI, Ollama, registry, strip_think (Wave 1)

**Wave 2** *(blocked on Wave 1 completion)*
- [x] 01-02-PLAN.md — Session + MCP: SessionManager SQLite WAL, McpClient, ToolRegistry, Dream stub, SkillsLoader stub (Wave 1, parallel)

**Wave 3** *(blocked on Wave 2 completion)*
- [x] 01-03-PLAN.md — AgentLoop + CLI: AgentLoop Nanobot, AutoCompact, command router, main.rs entrypoint, integration tests (Wave 2)

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

**Goal:** Adicionar todas as skills críticas que precisam ser Rust (performance, isolamento ou trait-driven): canais (Telegram + Webhook), hooks (guardrails + output-validator), persona system completo (roteador LLM + múltiplas personas paralelas + **Cabinet deliberativo**), **privacy tiers por persona**, memória sqlite (**contestável + core congelada + memory-flush**), **goal engine**, proactive engine (heartbeat + evento + idle).

**> Design crítico:** roteador e memória nascem **multi-owner-aware** mesmo que o family mesh (Phase 5) só ligue depois — princípio "architecture must not block". Roteador, Cabinet e privacy tiers são detalhados em `.planning/specs/cabinet-and-privacy-spec.md`.

**Requirements:**
CHAN-01, CHAN-02, CHAN-03, CHAN-04, PERS-01, PERS-02, PERS-03, PERS-04, PERS-05, PERS-06, PERS-07, CAB-01, CAB-02, CAB-03, CAB-04, CAB-05, PRIV-01, PRIV-02, PRIV-03, PRIV-04, MEM-01, MEM-02, MEM-03, MEM-04, MEM-05, MEM-06, MEM-07, MEM-08, MEM-09, GOAL-01, GOAL-02, GOAL-03, GOAL-04, HOOK-01, HOOK-02, HOOK-03, HOOK-04, HOOK-05, PROACT-01, PROACT-02, PROACT-03, PROACT-04, PROACT-05

**Plans:** 9 plans in 5 waves

Plans:
**Wave 1** *(foundation — traits + schema)*
- [x] 02-01-PLAN.md — Contestable memory: Memory trait, SQLite beliefs/provenance/goals, PrivacyTier, soft-revoke
- [x] 02-02-PLAN.md — Provider complete_structured + Hook/Observer traits + PrivacyEgressBlocked

**Wave 2** *(persona, hooks, goals — parallel)*
- [x] 02-03-PLAN.md — Persona: SOUL.md parser, registry, LLM router, runner (single/parallel)
- [x] 02-04-PLAN.md — Hooks: fail-closed egress, input guardrail, output-validator contestation, life-log
- [x] 02-05-PLAN.md — Goal engine: model, zero-LLM heuristic scoring, drift/confirm/replan

**Wave 3** *(deliberation + channels)*
- [x] 02-06-PLAN.md — Cabinet: mixed-tier downgrade, bounded deliberation, dissent-preserving synthesis
- [x] 02-07-PLAN.md — Channels: AgentHandle spine, Telegram (frankenstein), Webhook (axum)

**Wave 4** *(integration)*
- [x] 02-08-PLAN.md — AgentLoop wiring: route+hooks, pending_rx select arm, CronService, Dream, /as /cabinet /contest

**Wave 5** *(eval gate)*
- [x] 02-09-PLAN.md — Cargo-native eval harness: egress matrix + injection, revocation, dissent, proactive suppression

**Success Criteria:**
1. Mensagem chega via Telegram, roteador LLM seleciona persona correta entre as 8 disponíveis e responde
2. Comando `/as carreira <pergunta>` força persona específica
3. Solicitação que envolve duas áreas (ex: "estou ansioso com prazo de trabalho e atrasando treino") aciona personas em paralelo
4. **Decisão de alto peso / multi-domínio aciona o Cabinet: ≥2 personas deliberam e o agente entrega síntese (ou debate via `/cabinet`)**
5. **Persona `local-only` (ex: saúde) roteia exclusivamente a Ollama; guardrail bloqueia (fail-closed) qualquer tentativa de enviar seu contexto a provider cloud**
6. **Memória exibe proveniência ("acho X porque [sessão/data]"); usuário contesta e a crença antiga é revogada (peso → 0)**
7. **Meta anual registrada; nudge proativo sinaliza drift sem interromper sessão ativa (GOAL + PROACT-05)**
8. Heartbeat agendado dispara mensagem proativa em horário fixo via Telegram
9. Trigger de evento (calendar webhook) dispara comportamento proativo sem usuário pedir
10. Idle inferencístico (Dream) extrai fato e o usa em conversa futura sem pergunta explícita
11. Guardrails bloqueiam input malicioso antes de chegar ao LLM; output-validator filtra saída
12. Webhook recebe POST e responde com resposta do agente em JSON

**Anti-goals:**
- memupalace ainda não — memória usa só sqlite + tags (core congelada e proveniência implementadas sobre sqlite; enriquecidas em Phase 3)
- skill-writer ainda não — skills criadas manualmente (loop fechado SKWR-06 é Phase 3)
- Goal engine usa memória sqlite — scoring rico via memupalace fica para Phase 3
- Family mesh NÃO liga aqui — só o design não-bloqueante (multi-owner-aware)
- Bandit de modelo (INTEL-01) NÃO — fast-follow pós-uso real
- Sem Docker ainda — desenvolvimento local

---

## Phase 3: Python MCP Skill Servers

**Goal:** Os três skills Python (memupalace, skill-writer, self-improving) rodando como MCP servers isolados, cada um em seu container Docker com permissões próprias. memupalace funcionando como camada de memória rica (cache, knowledge graph, query sanitizer). skill-writer permite criar/editar skills só por conversa.

**Requirements:**
MCP-03, MUPL-01, MUPL-02, MUPL-03, MUPL-04, MUPL-05, MUPL-06, MUPL-07, SKWR-01, SKWR-02, SKWR-03, SKWR-04, SKWR-05, SKWR-06, SELF-01, SELF-02

**Success Criteria:**
1. memupalace MCP server roda em container isolado, embeddings ONNX local, sem acesso a API keys do core
2. Query sanitizer demonstrado: query com system prompt prepended retorna ≥70% recall (vs ~1% sem sanitizer)
3. Mario diz "toda sexta às 18h me lembra de revisar metas semanais" no Telegram → skill-writer cria SKILL.md, instala, ativa, e na sexta seguinte a skill dispara
4. Mario diz "no lembrete de metas, não me cobra antes das 9h" → skill-writer edita skill existente; comportamento ajusta
5. **Loop fechado (SKWR-06): após task complexa concluída, skill-writer destila o método numa skill reutilizável automaticamente; task parecida seguinte reusa a skill em vez de raciocinar do zero**
6. Audit do upstream mempalace executado e features novas incorporadas (registrado em `.planning/decisions/mempalace-audit.md`)
7. self-improving promote loop sugere melhoria com base em uso real

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

## Phase 5: v1.0 Cognitive Completion + Fabric-Ready Seams

<!-- formalizado 2026-06-13 a partir de BASTION-V1-COGNITIVE-SPEC.md (antes só no spec). Escopo aprovado: 6 itens do spec, sem M2. -->

**Goal:** Fechar o gap cognitivo revelado no soak do cutover (Phase 04): memória e skills estão conectadas via MCP mas **não integradas ao fluxo de resposta**. Integrar tools no runner/cabinet (BIG-1) e expor 3 seams genéricos — capabilities-com-escopo (#3 ≡ BIG-1), injeção de contexto (#2), eventos OTel (#4) — mais identidade-por-onboarding (M1), erro-visível-no-canal + `/logs` (M3) e higiene de concorrência. Resultado: Bastion conversa, **persiste memória e cria skill por NL no uso normal**, pronto para launch aberto no GitHub. Regra de fronteira: core = mecanismo (compor/rodar/injetar/observar); semântica do orquestrador (Katsui/Fabric) fica fora.

**Depends on:** Phase 4 (cutover-live)

**Requirements:** BIG-1, SEAM-2, SEAM-4, M1, M3, CONC-1
<!-- novos, soak-driven; SEAM-3 ≡ BIG-1 (não é REQ separado) -->

**Work items (ordenados por dependência):**
1. **BIG-1** (base, = seam #3) — runner/cabinet recebem o `CapabilityRegistry` e executam tool-loop; TODAS as tools passam pelo invoke genérico + gate de egress (WR-04) + aprovação; suporta register/unregister com escopo de turn. Entrega: **Contestable Memory** (store/recall/contestar via memupalace) + **skill-writer loop** (criar skill por NL).
2. **SEAM #2** — TurnContext provider: a montagem do system prompt aceita blocos de contexto opacos de um provider; core não interpreta o conteúdo.
3. **SEAM #4** — eventos de ciclo de vida (turn start / router_decision / tool_call / decisão / resultado / erro) em OpenTelemetry GenAI semantic conventions; sinks plugáveis (stdout/file/OTLP/webhook).
4. **M1** — identidade/voz por onboarding: bloco de ~1–2k escrito pelo agente no 1º uso, sempre injetado (via seam #2), editável por conversa, persistido pela camada de memória (depende de BIG-1). NÃO em bastion.toml.
5. **M3** — erro de turn → msg no canal ("tive um problema: X; veja /logs") em vez de silêncio; `/logs` (daemon + Telegram) mostra só ERROR/WARN recentes (ts+tipo+msg), nunca conteúdo.
6. **Higiene de concorrência (CONC-1)** — `PRAGMA busy_timeout=5000` no sqlite + session mutex (lock por session_id) no daemon.

**Plans:** 5/6 plans executed

Plans:
**Wave 1** *(base — parallel)*
- [x] 05-01-PLAN.md — BIG-1: CapabilityRegistry.remove()/list_tool_defs(), runner → complete(), tool-loop em run_turn_for via capability_registry.invoke, modelo llama-3.3-70b (Wave 1)
- [x] 05-02-PLAN.md — CONC-1: PRAGMA busy_timeout=5000 no init_schema + session mutex por owner no daemon_loop (Wave 1)

**Wave 2** *(dependem de BIG-1 ou independentes — parallel)*
- [x] 05-03-PLAN.md — SEAM #2: TurnContextProvider trait + ContextBlock, build_system_prompt com egress check por bloco (Wave 2, depende de 05-01)
- [x] 05-04-PLAN.md — M3: reply de erro no Telegram em vez de continue silencioso + comando /logs sem conteúdo (Wave 2, independente)
- [x] 05-05-PLAN.md — SEAM #4: OTel GenAI spans (invoke_agent / chat / execute_tool), init_otel_provider, stdout default + OTLP opt-in (Wave 2, depende de 05-01)

**Wave 3** *(depende de BIG-1 + SEAM #2)*
- [x] 05-06-PLAN.md — M1: IdentityProvider implementando TurnContextProvider, onboarding no 1º uso, injeção de identidade via SEAM #2 (Wave 3, depende de 05-01 + 05-03)

**Success Criteria:**
1. Em conversa normal (não só `run_provider_fallback`), o agente persiste e recupera memória — Contestable Memory store/recall/contestar funciona via memupalace
2. skill-writer loop: criar uma skill por linguagem natural numa conversa e ela fica disponível em uso subsequente
3. `CapabilityRegistry` passa todas as tools pelo invoke genérico com gate de egress (WR-04) + aprovação; register/unregister com escopo de turn funciona
4. SEAM #2: um bloco de contexto opaco de um provider é injetado no system prompt sem o core parsear sua semântica
5. M1: bloco de identidade escrito no onboarding do 1º uso, sempre injetado, editável por conversa, persistido fora do bastion.toml
6. SEAM #4: eventos emitidos em OTel GenAI semantic conventions, consumíveis por ≥1 sink (stdout/file/webhook) sem o core conhecer o consumidor
7. M3: erro de turn aparece no canal + `/logs` mostra só ERROR/WARN recentes sem conteúdo de conversa
8. Higiene: `busy_timeout=5000` aplicado + session mutex serializa turns por session_id

**Anti-goals:**
- M2 (limpeza de dead code v2) — follow-up, fora desta fase
- Modelo de objeto/ontologia, OCC/locks/timeline, Company Brain, Control Tower, semântica de ação de objeto, SafeGuard/Proxy/Context Engine — fica fechado/Fabric, NÃO no core
- Nenhuma semântica de orquestrador no core — só seams neutros

---

## Phase 6: Ecosystem & Mobile *(opcional, pós-v1.0)*
<!-- renumerado de Phase 5 → 6 (2026-06-13): Phase 5 agora = v1.0 Cognitive Completion, ver BASTION-V1-COGNITIVE-SPEC.md -->
<!-- Phase 5 (Cognitive Completion) definida no spec; não duplicada aqui pra evitar drift -->


**Goal:** Estender Bastion para fora do uso pessoal de Mario — companion app mobile, publicação de skills no agentskills.io, groundwork para Bastion Cloud.

**Requirements:** v2 (MOB-01, MOB-02, ECO-01, ECO-02, ECO-03, CHEX-01, CHEX-02, CHEX-03, MESH-01, MESH-02, MESH-03)

**Success Criteria:**
1. Flutter companion app lê webhook + SSE, exibe interações em tempo real
2. Pelo menos uma skill custom de Bastion publicada e instalada via agentskills.io
3. ClawHub migration path documentado e validado com pelo menos um skill migrado
4. Bastion Cloud arquitetura inicial documentada (não necessariamente implementada)
5. **Family mesh (MESH): duas instâncias Bastion compartilham memória seletiva (mercado/calendário) com personas privadas isoladas e fronteira de permissão por owner — habilitado pelo design multi-owner-aware da Phase 2**

**Anti-goals:**
- Phase 5 só inicia se Phases 1-4 estabilizarem em produção pessoal por ≥ 30 dias

**Plans:** 6/7 plans executed

Plans:
**Wave 1** *(foundation — unified connectivity layer)*
- [x] 06-01-PLAN.md — MeshTransport trait + allowlist + SSE /events + /mesh/ingest routes (Wave 1)

**Wave 2** *(parallel — depends on Wave 1)*
- [x] 06-02-PLAN.md — P2P impl (age E2E encrypt) + MeshSliceProvider + OTel mesh_sync + MESH-03 (Wave 2)
- [x] 06-03-PLAN.md — Flutter companion app: chat + cockpit + SSE + pairing (Wave 2)
- [x] 06-04-PLAN.md — ECO skills (agentskills publish/install) + ClawHub doc + Bastion Cloud doc + channel scaffold (Wave 2)

**Gap-closure Wave 1** *(parallel — CR-01/CR-02/CR-05 auth + schema fix; no shared files)*
- [x] 06-05-PLAN.md — JWT decode in resolve_owner_or_401 + OtcStore expose + Flutter sendMessage contract fix + fail-closed JWT secret (CR-01, CR-02, CR-05, WR-01)
- [x] 06-06-PLAN.md — privacy_tier column migration + store_belief/retrieve_tagged tier wire + write_cabinet_synthesis CloudOk + integration test (CR-03, CR-04)

**Gap-closure Wave 2** *(depends on 06-05 — shares webhook.rs + config.rs)*
- [ ] 06-07-PLAN.md — to_owner boundary check in P2PTransport + ingest_handler + append_mesh_peer rewrite with toml_edit + SSRF validation (CR-06, SEC-01, SEC-02, WR-02)

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
- **Diferenciação vs Hermes (2026-05-30):** wedge = Cabinet (CAB) + Privacy Tiers (PRIV) — "um gabinete de personas privadas; as sensíveis nunca saem da sua máquina". Inputs roubados do Hermes (core memory congelada, memory-flush, loop fechado skill-writer, bandit fast-follow) registrados em memória do projeto. Ver `.planning/specs/cabinet-and-privacy-spec.md`.
- **Multi-owner-aware é regra de design da Phase 2** — roteador + memória carregam `owner` desde já para não bloquear o family mesh (MESH, Phase 5).

---
*Created: 2026-05-10*
*Last updated: 2026-06-13 — Phase 5 planejada (6 plans, 3 waves: BIG-1/CONC-1 → SEAM#2/M3/SEAM#4 → M1)*
*Phase 6 gap-closure: 2026-06-17 — 3 gap plans (06-05..07) closing CR-01..CR-06, SEC-01, SEC-02 (SC#1 + SC#5 blocked)*
