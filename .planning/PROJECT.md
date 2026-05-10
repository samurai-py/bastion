# Bastion v3

## What This Is

Bastion é um runtime de agentes IA self-hosted, seguro e proativo, projetado primariamente para Mario — um usuário multitarefa que orquestra várias áreas da própria vida (carreira, saúde, negócio, estudos, projetos pessoais, idiomas, música, ativismo) — e por extensão para qualquer pessoa com perfil similar. v3 é uma reescrita completa do core (de Node/OpenClaw para Rust), preservando o ecossistema de skills/personas existentes e eliminando os pontos de dor que travam o uso diário: instalação frágil, falhas de permissão em containers e ausência de proatividade real.

## Core Value

**O Bastion deve ajudar Mario a fazer suas metas anuais avançarem** — sendo proativo, customizável apenas com linguagem natural, seguro por construção e instalável em minutos. Se tudo o mais falhar, esse impacto pessoal mensurável tem que sobreviver.

## Requirements

### Validated

<!-- Capacidades já entregues pela v2 (OpenClaw, Python) e que serão preservadas/migradas. -->

- ✓ **Sistema de personas múltiplas com SOUL.md** — 8 personas reais em uso (carreira, saúde, negocio, estudos, projetos-pessoais, idiomas, musica, ativismo) — v2
- ✓ **persona-engine** — carregamento/seleção de personas — v2 (Python)
- ✓ **guardrails** — validação de input/output — v2 (Python)
- ✓ **output-validator** — checagem de saída — v2 (Python)
- ✓ **weight-system** — sistema de pesos — v2 (Python)
- ✓ **self-improving** — promoção/aprendizado — v2 (Python)
- ✓ **bastion-calendar** — integração de calendário (Composio MCP) — v2
- ✓ **mcporter** — importação de MCP servers — v2
- ✓ **proactive-engine (heartbeat)** — disparos agendados — v2

### Active

<!-- Hipóteses de v3 — escopo a entregar. -->

- [ ] **Core Rust runtime** — AgentLoop, MessageBus, ToolRegistry, SkillsLoader, SessionManager (sqlite), command router, AutoCompact, Consolidator, Dream, pending queue, strip_think
- [ ] **Provider trait** — Anthropic, OpenAI, Ollama (OpenAI-compat) com hot-swap em runtime
- [ ] **MCP client integrado** — Composio como gateway primário + qualquer MCP server
- [ ] **Channel Telegram (built-in Rust)**
- [ ] **Channel Webhook (built-in Rust)** — preparação para mobile/web futuro
- [ ] **Hooks Rust** — guardrails + output-validator portados
- [ ] **CronService Rust** — proactive-engine + heartbeat
- [ ] **Memory trait Rust** — backend sqlite + integração Dream
- [ ] **Observer Rust** — life-log
- [ ] **Multi-persona com roteamento por LLM** — usuário envia mensagem, mini-LLM classifica intent e direciona para a persona correta; default por canal, override por comando (`/as <persona>`); execução em paralelo quando a solicitação exigir
- [ ] **Memória global tageada por persona** — banco único, fatos com tag de owner, leitura cruzada permitida, escrita com tag própria
- [ ] **Proatividade em 3 modos** — heartbeat agendado + trigger por contexto/evento + idle inferencístico (Dream); **explicitamente sem** intervenção mid-conversation
- [ ] **memupalace MCP server (Python)** — fusão memU (proativo, cache de insights, knowledge graph) + mempalace (wing/room, query sanitizer, semantic search)
- [ ] **skill-writer MCP server (Python)** — cria E edita skills por linguagem natural; depende de memU para padrões/contexto
- [ ] **self-improving MCP server (Python)** — porta da skill atual
- [ ] **Container `FROM scratch`** — core binary + SKILL.md only; MCP servers Python em containers separados com permissões próprias
- [ ] **bastion.toml único** — configuração com defaults sensatos
- [ ] **Installer one-line** — `bash <(curl -fsSL https://bastion.run/install)`; falhas resolvidas pelo Claude Code automaticamente
- [ ] **Token cost optimization first-class** — AutoCompact, Consolidator, progressive tool loading, memU cache, Ollama path para custo zero, daily budget cap (`DAILY_BUDGET_USD`), Dream idle distillation
- [ ] **Cutover v2 → v3** — Mario para de usar v2 quando Phase 4 entrega (Docker scratch + installer). Personas e skills serão **reescritas** se a v3 simplificar (compatibilidade total NÃO é requisito)

### Out of Scope

<!-- Decisões explícitas de exclusão para v3. -->

- **21 messaging channels built-in (estilo ZeroClaw)** — ship Telegram + Webhook; comunidade adiciona outros via `/add-<channel>` skills
- **Hardware peripherals (robot-kit, STM32, RPi GPIO)** — fora do escopo; Bastion é agente, não robô
- **Enterprise security model (pairing ceremony, multi-level estop, policy engine)** — overkill para usuário solo; segurança via isolamento de container + ausência de raw API keys nos agentes
- **BMAD/enterprise planning overhead** — GSD é o framework de planejamento
- **Voice (STT/TTS)** — não em v3
- **Computer use / screen automation** — não em v3
- **Web UI / dashboard** — Telegram é a UI; webhook prepara mobile futuro
- **Multi-tenant SaaS / Bastion Cloud** — produto separado, fora do escopo de v3
- **Suporte a todos LLM providers** — Anthropic + OpenAI-compat cobre tudo (Ollama, Groq, OpenRouter, etc.)
- **Compatibilidade automática com personas/skills v2** — aceita-se reescrita se v3 simplificar formato
- **Intervenção proativa mid-conversation** — proatividade ocorre fora da sessão (heartbeat/evento/idle), não interrompe diálogo ativo
- **Flutter app / IPA / APK** — Phase 5+ opcional, não é parte de v3
- **agentskills.io publishing / ClawHub migration / Bastion Cloud groundwork** — pós-v3
- **Aceitação de PRs externos amplos** — projeto é público mas source-available com licença restritiva (estilo BSL/Polyform Strict); contribuições só com aprovação explícita

## Context

**Projeto pessoal-first com generalização planejada.** Bastion v3 sucede uma versão funcional (v2/OpenClaw) que Mario já usa. Existe legado real:

- **Personas vivas em `personas/`** com SOUL.md por persona; algumas com `memory.md` populado (carreira, negocio).
- **Skills Python em `skills/`** já implementadas (persona-engine, guardrails, weight-system, output-validator, self-improving, bastion-calendar, mcporter).
- **Codebase map disponível** em `.planning/codebase/` (ARCHITECTURE.md, STACK.md, CONVENTIONS.md, STRUCTURE.md, TESTING.md, INTEGRATIONS.md, CONCERNS.md).
- **STRATEGY.md na raiz** — documento vivo com decisões de arquitetura, fases, métricas e não-objetivos.

**Inspirações arquiteturais explicitamente adotadas:**
- **Nanobot (HKUDS)** — AgentLoop pattern, AutoCompact, Consolidator, Dream, pending queue, hot-swap, strip_think, command router.
- **NanoClaw (qwibitai)** — isolamento OS-level, agentes sem raw API keys, codebase auditável, channels via skill (não built-in).
- **ZeroClaw** — trait system inspiration; explicitamente **descartado**: 21 channels, robot-kit, enterprise security.
- **memU (NevaMind-AI)** + **mempalace (mempalace/mempalace)** — fundidos em "memupalace" (decisão deliberada, ver Key Decisions).

**Dor primária do OpenClaw que justifica o rewrite:** falhas de permissão em containers (Node/Python rodando como root, pip permission hell, volumes com perm errada). É insegurança e fragilidade — não vaidade tecnológica.

**Métricas de sucesso pessoal (não técnicas):** Mario saberá que v3 cumpriu seu propósito quando suas **metas anuais avançarem** (carreira, saúde, projetos) e ele puder atribuir parte do progresso ao Bastion (cobranças, lembretes, organização).

## Constraints

- **Tech stack**: Rust core (tokio async, trait-driven), Python para MCP servers (memupalace, skill-writer, self-improving), Markdown puro para skills declarativas. Sem Node.js no core.
- **Performance**: binário ≤ 20MB, cold start ≤ 100ms, Docker image core ≤ 50MB.
- **Instalação**: `docker compose up` após preencher `.env`, total under 5 minutos do zero ao primeiro `/start` no Telegram.
- **Segurança**: zero raw API keys nos agentes; containers Python isolados com permissões próprias; FROM scratch para o core.
- **Compatibilidade**: SKILL.md continua compatível com ecossistema Nanobot/OpenClaw/agentskills.io; SOUL.md (personas) pode mudar de formato se v3 simplificar.
- **Token cost**: gerenciamento de custo é decisão de arquitetura, não otimização tardia. Daily budget cap configurável. Caminho Ollama para custo zero.
- **Distribuição**: público no Git, source-available, contribuições restritas (Mario aprova).
- **Linguagem**: pt-BR como idioma primário do usuário e personas (sistema deve suportar i18n; skills atuais já têm `i18n.py`).

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Reescrever core em Rust (substituir OpenClaw/Node) | Eliminar pip permission hell, tamanho de imagem, startup lento; viabilizar `FROM scratch` | — Pending |
| Adotar AgentLoop pattern do Nanobot (HKUDS) | Padrão validado: MessageBus → context build → LLM → tool exec → save; reduz invenção arquitetural | — Pending |
| Adotar isolamento OS-level do NanoClaw (qwibitai) | Agentes sem raw API keys; container = unidade de isolamento real | — Pending |
| Telegram + Webhook como únicos canais built-in | Resto via skills `/add-<channel>`; reduz superfície de manutenção | — Pending |
| Providers: Anthropic + OpenAI + Ollama (OpenAI-compat) | Cobre cloud + local; Ollama destrava custo zero para self-host | — Pending |
| Composio como MCP gateway primário | Acesso a centenas de APIs sem manter integrações próprias | — Pending |
| memupalace = memU + mempalace (fusão deliberada) | memU traz proatividade/cache, mempalace traz query sanitizer (recupera retrieval de 1% para 70-89%) e wing/room taxonomy | — Pending |
| Multi-persona com roteamento por LLM + memória global tageada | Conversa flui sem comandos; tags evitam vazamento, permitem leitura cruzada controlada | — Pending |
| Proatividade em 3 modos, **sem** intervenção mid-conversation | Heartbeat, evento e idle cobrem o espectro útil; interromper sessão é antipattern UX | — Pending |
| skill-writer fica em Phase 3 (depende de memU) | Sem memória de padrões prévios, skill-writer seria burro; aceitar que cutover só vale após Phase 3+4 | — Pending |
| Cutover v2 → v3 só na Phase 4 (após Docker scratch + installer) | Mario continua em v2 até v3 ser plug-and-play; reduz risco de migração incompleta | — Pending |
| Personas/skills v2 podem ser reescritas em v3 | Compatibilidade total não vale o custo; Mario é o único usuário hoje, ganho de simplicidade prevalece | — Pending |
| Source-available com licença restritiva (estilo BSL/Polyform Strict) | Código público para auditoria e visibilidade; controle de contribuição mantido | — Pending |
| Phases sequenciais, não milestones — Phase 1 é big rewrite atômico | Tentar entregar incrementalmente um core que substitui OpenClaw geraria interfaces fantasma; rewrite atômico é mais honesto | — Pending |
| GSD (este framework) como orquestrador de planning/execução | Mario já adotou; reduz peso de processo customizado | ✓ Good |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-05-10 after initialization*
