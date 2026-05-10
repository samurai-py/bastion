# Requirements: Bastion v3

**Defined:** 2026-05-10
**Core Value:** Bastion ajuda Mario a fazer suas metas anuais avançarem — proativo, customizável por linguagem natural, seguro e instalável em minutos.

## v1 Requirements

Requisitos de v3 (release inicial do runtime Rust). Cada um mapeia para exatamente uma fase no ROADMAP.md.

### Core Runtime (CORE)

- [ ] **CORE-01**: AgentLoop assíncrono (tokio) executa MessageBus → context build → LLM call → tool exec → session save
- [ ] **CORE-02**: ToolRegistry gerencia tools registradas em runtime com progressive loading (nomes em contexto, schemas sob demanda)
- [ ] **CORE-03**: SkillsLoader carrega skills de filesystem (SKILL.md + Rust trait impls) e suporta hot-reload
- [ ] **CORE-04**: SessionManager persiste sessões em sqlite com TTL e recuperação após restart
- [ ] **CORE-05**: AutoCompact comprime sessão antes de atingir limite de contexto
- [ ] **CORE-06**: Consolidator faz compressão de contexto baseada em tokens
- [ ] **CORE-07**: Dream extrai fatos duráveis durante idle e injeta em memória futura
- [ ] **CORE-08**: Pending queue permite injeção de mensagens mid-turn sem interromper diálogo ativo
- [ ] **CORE-09**: strip_think remove blocos `<think>` antes de armazenar/exibir
- [ ] **CORE-10**: Command router processa comandos de sistema (`/stop`, `/model`, `/as <persona>`, etc.)
- [ ] **CORE-11**: CLI `bastion agent -m "..."` executa um turno e imprime resposta
- [ ] **CORE-12**: CLI `bastion daemon` inicia processo de longa duração com canais ativos

### Provider Layer (PROV)

- [ ] **PROV-01**: Trait `Provider` define interface comum para chamadas LLM
- [ ] **PROV-02**: Provider Anthropic implementa Claude (Messages API)
- [ ] **PROV-03**: Provider OpenAI implementa GPT (Chat Completions API)
- [ ] **PROV-04**: Provider Ollama implementa modelos locais via API OpenAI-compat
- [ ] **PROV-05**: Hot-swap de provider em runtime via comando `/model` sem restart
- [ ] **PROV-06**: Daily budget cap configurável (`DAILY_BUDGET_USD`) bloqueia chamadas cloud quando excedido

### MCP Integration (MCP)

- [ ] **MCP-01**: Cliente MCP genérico conecta a qualquer servidor MCP via stdio/sse
- [ ] **MCP-02**: Composio configurado como gateway primário para APIs externas
- [ ] **MCP-03**: MCP servers Python rodam em containers Docker isolados com permissões próprias

### Channels (CHAN)

- [ ] **CHAN-01**: Channel trait define interface para canais de entrada/saída
- [ ] **CHAN-02**: Telegram channel built-in (Rust) suporta mensagens, comandos, mídia básica
- [ ] **CHAN-03**: Webhook channel built-in (Rust) recebe e envia eventos HTTP arbitrários
- [ ] **CHAN-04**: Roteamento por canal: cada canal tem persona default configurável

### Persona System (PERS)

- [ ] **PERS-01**: Persona definida por SOUL.md (formato pode evoluir vs v2; reescrita aceita)
- [ ] **PERS-02**: Múltiplas personas coexistem no mesmo runtime
- [ ] **PERS-03**: Persona tem conjunto próprio de skills habilitadas
- [ ] **PERS-04**: Roteador LLM classifica intent da mensagem e seleciona persona apropriada
- [ ] **PERS-05**: Override por comando `/as <persona>` força persona específica
- [ ] **PERS-06**: Múltiplas personas podem responder em paralelo quando solicitação exige
- [ ] **PERS-07**: Persona-engine carrega definições de `personas/<name>/SOUL.md`

### Memory (MEM)

- [ ] **MEM-01**: Trait `Memory` define interface para backends de memória
- [ ] **MEM-02**: Backend sqlite implementa Memory com store/retrieve/search
- [ ] **MEM-03**: Memória global tageada por persona (banco único, tag de owner)
- [ ] **MEM-04**: Personas podem ler memória cruzada; escrita marca tag própria
- [ ] **MEM-05**: Integração com Dream para distillation periódica

### Memupalace (MUPL)

- [ ] **MUPL-01**: memupalace é MCP server Python isolado em container próprio
- [ ] **MUPL-02**: Ingere insights via memU pattern (cache de fatos, evita LLM redundante)
- [ ] **MUPL-03**: Knowledge graph hierárquico (categorias → items → cross-references)
- [ ] **MUPL-04**: Wing/room taxonomy (mempalace) para organização semântica
- [ ] **MUPL-05**: Query sanitizer remove system prompt prepended (recupera retrieval ~70-89%)
- [ ] **MUPL-06**: Embeddings ONNX local (sem dependência cloud para retrieval)
- [ ] **MUPL-07**: Auditoria de features novas no upstream mempalace antes de Phase 3

### Skill-Writer (SKWR)

- [ ] **SKWR-01**: skill-writer é MCP server Python isolado
- [ ] **SKWR-02**: Cria nova skill (SKILL.md + arquivos de suporte) a partir de descrição em linguagem natural
- [ ] **SKWR-03**: Edita skill existente conforme conversa ('na próxima vez não me cobra antes das 9h')
- [ ] **SKWR-04**: Versiona skills geradas (histórico, rollback)
- [ ] **SKWR-05**: Aprende padrões do usuário via memupalace para gerar skills mais alinhadas

### Self-Improving (SELF)

- [ ] **SELF-01**: self-improving é MCP server Python (port da skill v2)
- [ ] **SELF-02**: Promote loop sugere melhorias com base em uso

### Hooks & Observers (HOOK)

- [ ] **HOOK-01**: Hook trait permite interceptação de input/output
- [ ] **HOOK-02**: guardrails portado para Rust (validação de input)
- [ ] **HOOK-03**: output-validator portado para Rust (checagem de saída)
- [ ] **HOOK-04**: Observer trait permite registro passivo de eventos
- [ ] **HOOK-05**: life-log Observer registra histórico de interações para análise

### Proactive Engine (PROACT)

- [ ] **PROACT-01**: CronService Rust agenda tarefas periódicas (heartbeat diário, semanal, etc.)
- [ ] **PROACT-02**: proactive-engine porta heartbeat scheduler de v2
- [ ] **PROACT-03**: Trigger por evento: webhook/calendar/file watcher dispara comportamento proativo
- [ ] **PROACT-04**: Idle inferencístico: durante ociosidade, agente avalia memória e decide se há algo a comunicar
- [ ] **PROACT-05**: Proatividade NÃO interrompe sessão ativa (apenas heartbeat / evento / idle)

### Packaging & Install (PKG)

- [ ] **PKG-01**: Dockerfile `FROM scratch` para core (binário estático + SKILL.md only)
- [ ] **PKG-02**: docker-compose.yml define core + MCP servers Python isolados
- [ ] **PKG-03**: Imagem core ≤ 50MB; binário ≤ 20MB; cold start ≤ 100ms
- [ ] **PKG-04**: `bastion.toml` único com defaults sensatos
- [ ] **PKG-05**: Installer one-line `bash <(curl -fsSL https://bastion.run/install)` resolve dependências
- [ ] **PKG-06**: Falhas no installer disparam Claude Code para diagnose/resume (modelo NanoClaw)
- [ ] **PKG-07**: Setup completo `docker compose up` em ≤ 5 minutos do zero ao primeiro `/start` no Telegram
- [ ] **PKG-08**: Zero permission fixes manuais em containers
- [ ] **PKG-09**: SKILL.md compatibilidade verificada vs formato agentskills.io

## v2 Requirements

Adiados para pós-v3 (Phase 5+). Tracked mas fora do escopo atual.

### Mobile (MOB)

- **MOB-01**: Flutter companion app (webhook + SSE)
- **MOB-02**: Native APK/IPA distribuição

### Ecosystem (ECO)

- **ECO-01**: agentskills.io publishing pipeline
- **ECO-02**: ClawHub migration path
- **ECO-03**: Bastion Cloud groundwork (managed deployment)

### Channels Extended (CHEX)

- **CHEX-01**: WhatsApp channel via skill `/add-whatsapp`
- **CHEX-02**: Discord channel via skill `/add-discord`
- **CHEX-03**: Email channel via skill `/add-email`

## Out of Scope

Explicitamente excluídos. Documentado para evitar scope creep.

| Feature | Reason |
|---------|--------|
| 21 messaging channels built-in (estilo ZeroClaw) | Escopo demais; comunidade adiciona via skills `/add-<channel>` |
| Hardware peripherals (robot-kit, STM32, RPi GPIO) | Bastion é agente, não robô |
| Enterprise security model (pairing ceremony, multi-level estop, policy engine) | Overkill para usuário solo; segurança via container isolation + sem raw API keys |
| BMAD/enterprise planning overhead | GSD é o framework de planning |
| Voice (STT/TTS) | Não em v3 |
| Computer use / screen automation | Não em v3 |
| Web UI / dashboard | Telegram é a UI; webhook prepara mobile futuro |
| Multi-tenant SaaS | Bastion Cloud é produto separado, fora de v3 |
| Suporte a todos LLM providers | Anthropic + OpenAI-compat cobre tudo (Ollama, Groq, OpenRouter) |
| Compatibilidade automática com personas/skills v2 | Reescrita aceita se v3 simplifica |
| Intervenção proativa mid-conversation | Antipattern UX; proatividade só fora da sessão |
| Aceitação ampla de PRs externos | Source-available com licença restritiva (BSL/Polyform); contribuições só com aprovação de Mario |

## Traceability

Mapeamento de requisitos para fases. Atualizado durante criação do roadmap.

| Requirement | Phase | Status |
|-------------|-------|--------|
| CORE-01 | Phase 1 | Pending |
| CORE-02 | Phase 1 | Pending |
| CORE-03 | Phase 1 | Pending |
| CORE-04 | Phase 1 | Pending |
| CORE-05 | Phase 1 | Pending |
| CORE-06 | Phase 1 | Pending |
| CORE-07 | Phase 1 | Pending |
| CORE-08 | Phase 1 | Pending |
| CORE-09 | Phase 1 | Pending |
| CORE-10 | Phase 1 | Pending |
| CORE-11 | Phase 1 | Pending |
| CORE-12 | Phase 1 | Pending |
| PROV-01 | Phase 1 | Pending |
| PROV-02 | Phase 1 | Pending |
| PROV-03 | Phase 1 | Pending |
| PROV-04 | Phase 1 | Pending |
| PROV-05 | Phase 1 | Pending |
| PROV-06 | Phase 1 | Pending |
| MCP-01 | Phase 1 | Pending |
| MCP-02 | Phase 1 | Pending |
| CHAN-01 | Phase 2 | Pending |
| CHAN-02 | Phase 2 | Pending |
| CHAN-03 | Phase 2 | Pending |
| CHAN-04 | Phase 2 | Pending |
| PERS-01 | Phase 2 | Pending |
| PERS-02 | Phase 2 | Pending |
| PERS-03 | Phase 2 | Pending |
| PERS-04 | Phase 2 | Pending |
| PERS-05 | Phase 2 | Pending |
| PERS-06 | Phase 2 | Pending |
| PERS-07 | Phase 2 | Pending |
| MEM-01 | Phase 2 | Pending |
| MEM-02 | Phase 2 | Pending |
| MEM-03 | Phase 2 | Pending |
| MEM-04 | Phase 2 | Pending |
| MEM-05 | Phase 2 | Pending |
| HOOK-01 | Phase 2 | Pending |
| HOOK-02 | Phase 2 | Pending |
| HOOK-03 | Phase 2 | Pending |
| HOOK-04 | Phase 2 | Pending |
| HOOK-05 | Phase 2 | Pending |
| PROACT-01 | Phase 2 | Pending |
| PROACT-02 | Phase 2 | Pending |
| PROACT-03 | Phase 2 | Pending |
| PROACT-04 | Phase 2 | Pending |
| PROACT-05 | Phase 2 | Pending |
| MCP-03 | Phase 3 | Pending |
| MUPL-01 | Phase 3 | Pending |
| MUPL-02 | Phase 3 | Pending |
| MUPL-03 | Phase 3 | Pending |
| MUPL-04 | Phase 3 | Pending |
| MUPL-05 | Phase 3 | Pending |
| MUPL-06 | Phase 3 | Pending |
| MUPL-07 | Phase 3 | Pending |
| SKWR-01 | Phase 3 | Pending |
| SKWR-02 | Phase 3 | Pending |
| SKWR-03 | Phase 3 | Pending |
| SKWR-04 | Phase 3 | Pending |
| SKWR-05 | Phase 3 | Pending |
| SELF-01 | Phase 3 | Pending |
| SELF-02 | Phase 3 | Pending |
| PKG-01 | Phase 4 | Pending |
| PKG-02 | Phase 4 | Pending |
| PKG-03 | Phase 4 | Pending |
| PKG-04 | Phase 4 | Pending |
| PKG-05 | Phase 4 | Pending |
| PKG-06 | Phase 4 | Pending |
| PKG-07 | Phase 4 | Pending |
| PKG-08 | Phase 4 | Pending |
| PKG-09 | Phase 4 | Pending |

**Coverage:**
- v1 requirements: 69 total
- Mapped to phases: 69
- Unmapped: 0 ✓

---
*Requirements defined: 2026-05-10*
*Last updated: 2026-05-10 after initial definition*
