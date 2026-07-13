# Bastion Revamp — Backlog (v2.0)

> Reorganização do Bastion em **substrato reutilizável** (família de crates) + **produto pessoal** (Bastion Agent), com runtime de agentes externos (`AgentRuntime`) substituindo o terminal-agent legado, protocolo de extensões, backends por assinatura e split físico de repositórios ao final. Versão alvo: **v2.0**.
>
> Regra de ouro: **não reescrever enquanto separa**. Comportamento preservado primeiro, redesign depois. Nenhuma extração física antes do boundary provado no workspace.
>
> Convenções: `[ ]` pendente · `[~]` em progresso · `[x]` feito. Decisões de escopo resolvidas em 2026-07-13 — ver seção **Decisões** no fim.

---

## 0. Resultado esperado

1. `bastion-agent` e um host embedded mínimo executam turns pelas mesmas APIs públicas do substrato.
2. Substrato não importa módulos do Agent nem conceitos de consumidores fechados; Agent não importa código de terceiros fechados.
3. Invariantes de egress, trust, approval, owner/session e tool invocation permanecem testadas.
4. Cognição compartilhada (personas, memória, Dream, learning, goals, proatividade, deliberação) continua reutilizável — não vira feature exclusivamente pessoal.
5. Split físico de repositórios só após separação lógica provada no workspace.
6. Protocolo de extensões suporta componentes fora do processo sem ABI dinâmica Rust e sem bypass do `CapabilityRegistry`.
7. Agent instala uma Experience multi-extension, resolve um Loadout reproduzível e faz upgrade/rollback seguro.

### Regras de dependência (CI valida, não review humano)

```text
bastion-agent ───────────────► crates Bastion
host embedded externo ───────► crates Bastion
extensões Bastion ───────────► kernel Bastion

PROIBIDO:
kernel ─► agent
kernel/extensões ─► consumidor externo
agent ─► consumidor externo (e vice-versa)
```

---

## A. Trilha transversal — AgentRuntime (substitui terminal-agent)

O provider terminal-agent atual (`claude -p`/OpenCode via stdout) é ponte de compatibilidade: achata mensagens em prompt, não preserva sessão/eventos, retorna `tool_calls: None` e deixa tools internas do CLI escaparem de egress/approval/budget. Substituição por duas abstrações:

| Abstração | Responsabilidade |
|---|---|
| `ModelProvider` | inferência/streaming/tool-use de uma chamada |
| `AgentRuntime` | sessão estruturada cujo harness externo possui o loop interno (terminal, arquivos, tools, artefatos) |

Três modos operacionais: (1) conversa por inferência nativa — Bastion possui o tool loop; (2) conversa primária runtime-backed — harness executa todos os turns, Bastion possui o envelope (identidade, memória, canais, supervisão); (3) tarefa delegada — conversa continua enquanto sessão longa devolve eventos/artefatos/resultado.

**Ordem:** A-01/A-02 rodam em paralelo a M1 (são contrato+testes, não conflitam com inventário). A-03 em diante só depois de M2, pra nascerem na crate certa (`bastion-agent-runtime`).

- [x] **A-01** Contrato `AgentRuntime` + threat model (draft em `docs/revamp/A-01-agentruntime-contract.md`): `start`/`resume`, `run_task`, eventos tipados, `steer`, `cancel`, status, timeout, workspace, sandbox, permission profile, approvals, usage, artefatos, correlação OTel.
- [x] **A-02** (`src/agent_runtime/conformance.rs` + FakeRuntime, 14/14 checks) Conformance suite comum (antes dos adapters): start/resume/steer/cancel/timeout/queue/streaming/diff+artefatos/permission profile/restart+crash recovery/OTel/auth-profile ref + declaração honesta de quais ações passaram pelo registry vs. ocorreram no sandbox externo. Teste negativo: rejeita stdout humano/ANSI e adapter incompatível; falha do client não corrompe sessão Bastion.
- [ ] **A-03** `CodexAppServerRuntime` nativo (Codex app-server): lifecycle, auth existente, sessões, eventos, steer/cancel, usage, artifacts.
- [ ] **A-04** `AcpxAgentRuntime`: processo `acpx` supervisionado, JSON-RPC/NDJSON `json-strict`, version pin, health/doctor. Tipos/paths/lifecycle do acpx não vazam pra API pública; cliente ACP Rust pode substituí-lo depois sem mudar `BackendProfile`.
- [ ] **A-05** Mesma conformance nos dois adapters + Codex nativo vs. Codex-via-ACP (prova que o contrato não foi moldado a uma implementação).
- [ ] **A-06** Runtime-backed **primary conversation**: Codex, Claude Code e OpenCode autenticados no host como `conversation_backend` de todos os turns.
- [ ] **A-07** Runtime-backed **delegated task**: `task_runtime` com conversa concorrente, cancelamento, retomada e restart recovery.
- [ ] **A-08** Security/live E2E + matriz versionada de targets, capabilities, auth e policy coverage. *(Absorve o antigo UAT-02: o legado não será validado, será substituído.)*
- [ ] **A-09** Terminal-agent → feature `legacy-terminal-agent` → remoção após 1 release de deprecation (gate: A-05 + A-08 verdes, rollback legado testado).

## M0. Baseline congelada (mínima)

Decisão: **sem validação ao vivo neste marco** — os débitos live herdados da v1.1 (UAT-01, FLUT-01, SO-05, canal externo) descem pra M7/uso real; UAT-02 morre absorvido pela Trilha A (A-08).

- [x] **M0-01** Tag imutável `v1.1.0-pre-revamp` (→ 1528759) + métricas em `docs/revamp/BASELINE.md` (525 testes, binário 24MB ⚠️>20MB, 33k LOC, 283 pub).
- [x] **M0-02** Gates verdes: fmt limpo, clippy exit 0 (future-incompat de dep anotado), 525 testes.
- [x] **M0-03** Inventário `keep|move|shim|delete-later` em `docs/revamp/LEGACY-INVENTORY.md` (3 itens `?` pra resolver antes do M6).

## M1. Boundaries antes de mover código

- [ ] **M1-01** ADR "substrate, cognition and product split".
- [ ] **M1-02** Inventário módulo→destino confirmado por análise de dependências (hipótese abaixo).
- [ ] **M1-03** Grafo de dependências real + ciclos que impedem separação.
- [ ] **M1-04** Lista de APIs públicas mínimas a estabilizar: `Runtime::run_turn(TurnRequest) -> TurnResult`; `Capability`/`CapabilityRegistry`/`InvokeContext`; `ContextProvider`/`ContextBlock`; `SessionStore`; `Provider`; `Observer`/event contract; ports opcionais de approval/budget/policy; `AgentDefinition` + bindings; `Memory`/`Belief`/proveniência; learning delta + interop; `ExtensionManifest`/`PackManifest` + lifecycle + permissões; `Loadout` resolvido + lockfile; delegação de subagente + ownership de agente coletivo; `AgentRuntime`; `AuthProfileRef`; `VersionedContextArtifact`/`ContextRevision`; `DeliberationStrategy` + Cabinet (contrato estável — decisão #6).
- [ ] **M1-05** Matriz mechanism/policy: o que é mecanismo OSS compartilhado vs. política do Agent vs. política de host externo.
- [ ] **M1-06** Política de estabilidade por crate (kernel semver estrito; cognição `0.x` exceto Cabinet; produto sem promessa de lib).
- [x] **M1-07** Caracterização das invariantes (mapa em `docs/revamp/M1-07-characterization-map.md`; 5 testes novos em `tests/characterization_boundary.rs`; 1 gap estrutural documentado).

Topologia alvo (decisão #1 — intermediária, 10 crates + app; confirmar destino fino em M1-02):

| Crate | Conteúdo | Cadência |
|---|---|---|
| `bastion-types` | tipos folha, mensagens, IDs, erros | kernel |
| `bastion-runtime` | agent loop, capabilities, context, sessions, hooks, observabilidade | kernel |
| `bastion-memory` | traits, beliefs, temporalidade, contestação, store | quase-kernel |
| `bastion-cognition` | Dream, procedural/learning, goals, proatividade, **Cabinet (estável)** | evolutiva |
| `bastion-personas` | `AgentDefinition` 0.x + bindings; promove com 2º consumidor (decisão #5) | evolutiva |
| `bastion-mesh` | mesh, identity, interop (transporte neutro) | evolutiva |
| `bastion-mcp` | MCP client/server | evolutiva |
| `bastion-agent-runtime` | contrato `AgentRuntime` + adapters (Codex, acpx) + terminal-agent legado até A-09 | evolutiva |
| `bastion-extension-protocol` | manifests, lifecycle, permissões, SDK | evolutiva |
| `bastion-providers` | providers concretos (Anthropic/OpenAI/Ollama/Gemini/Groq/OpenRouter) + `AuthProfileRef`/auth (decisão #4) | evolutiva |
| `bastion-agent` (app) | daemon, channels concretos, api, config, installer, mobile, UX | produto |

## M2. Separação lógica no workspace

- [x] **M2-01** Workspace criado (`f0f6650`); binário intacto (+576 bytes vs baseline).
- [x] **M2-02** `bastion-types` extraída via git mv + shim de re-export (`ec30069`); 533 testes verdes.
- [ ] **M2-03** Extrair runtime (capabilities/context/sessions/hooks/observabilidade).
- [ ] **M2-04** Extrair memory + provider traits.
- [ ] **M2-05** Extrair agent loop completo.
- [ ] **M2-06** Extrair cognition/personas/mesh/mcp como extensões.
- [ ] **M2-07** Binário atual vira composição das crates.
- [ ] **M2-08** CI de dependências proibidas (regras da seção 0).

Regras de migração: commits pequenos por boundary; comportamento preservado antes de redesign; re-exports temporários com data de remoção; zero rename cosmético misturado à extração; medir binário/performance a cada marco.

Gate: workspace compila por crate e como produto; kernel compila sem features de Agent; sem ciclos kernel↔cognition↔app; testes de caracterização da baseline verdes.

## M3. Substrato como biblioteca

Distribuição durante incubação: **path deps no workspace + git deps version-pinned para consumidores externos; crates.io só no M6** (decisão #3).

- [ ] **M3-01** Reduzir `pub` ao contrato; erros tipados fail-closed nas bordas.
- [ ] **M3-02** Documentar invariantes de segurança: toda invocation passa pelo registry; `privacy_tier` ausente não vira allow; conteúdo não confiável não ganha autoridade; approval não bypassável por adapter; sessions owner-scoped; agente nunca recebe SQL cru.
- [ ] **M3-03** Compatibility tests contra a versão anterior suportada; checagem de API breaking no CI.
- [ ] **M3-04** Exemplos `minimal-agent` e `embedded-host` sem dependência do produto.
- [ ] **M3-05** Feature flags + matriz de combinações suportadas; build mínimo sem features de produto.
- [ ] **M3-06** Política de semver, MSRV, depreciação; docs de embedding/upgrade. Licença: **source-available restritiva em todas as crates** (decisão #10).
- [ ] **M3-07** `bastion-extension-protocol` + SDK — **os três mecanismos no primeiro release** (decisão #11): (1) artefatos declarativos; (2) WASM/WASI; (3) processo externo com protocolo versionado. Crate estática permanece caminho para extensão oficial/host embedded. Nunca ABI dinâmica Rust como padrão comunitário.
- [ ] **M3-08** Conformance de extensão nos três mecanismos: handshake, lifecycle, eventos, health, permissions, secrets, shutdown. Extensão de referência passa sem acesso implícito a processo/secrets/filesystem.
- [ ] **M3-09** `ExtensionManifest`/`PackManifest` verificáveis: publisher/id, versão, compatibilidade, provides/requires, permissões, egress, devices, secrets, entrypoint, migrations, policy coverage. Assinatura + trust tier `official|verified|community|local`.
- [ ] **M3-10** Conformance de auth: login/refresh/revogação/quarantine/owner scope. Refresh tokens fora de config, log, export, prompt e definição de agente.
- [ ] **M3-11** `ContextRevision`: atualização só no boundary entre turns; estratégia explícita para revision stale (última válida ou fail-closed).

Gate: programa externo constrói e executa um turn só pela API documentada; security tests falham ao introduzir bypass conhecido; host externo implementa o protocolo de extensão sem ABI Rust.

## M4. Bastion Agent como produto

- [ ] **M4-01** App `bastion-agent` no workspace: daemon, canais concretos, config, installer, mobile, UX.
- [ ] **M4-02** `PersonalAgentPolicy` (memória, approval, routing, Dream, goals, proatividade).
- [ ] **M4-03** **Sem migração automática** (decisão #8): instalação v1.1 do owner é migrada manualmente uma vez; nenhum código de migrator entra no produto. Export/import `.af` continua sendo o caminho de portabilidade.
- [ ] **M4-04** Posicionamento público: *agente pessoal longitudinal, contestável e authority-safe*.
- [ ] **M4-05** UX de diferenciação: fonte/validade de memória; correção/revogação; approvals pendentes; confiança/origem de conteúdo; local/cloud por tier; exportação/portabilidade.
- [ ] **M4-06** `BackendProfile`: `conversation_backend: ModelProvider|AgentRuntime`, `task_runtime: AgentRuntime?`, auth por backend, model/runtime id, permission+workspace policy, policy-coverage declaration por modo. UI distingue "Bastion tool loop" vs "harness tool loop".
- [ ] **M4-07** Login por assinatura como backend de primeira classe: Codex/ChatGPT (app-server), Claude (Claude Code/Agent SDK), Cursor (ACP), OpenCode (ACP + providers próprios). Instalação pessoal funciona **sem API key** quando há assinatura suportada; API tradicional continua suportada, nunca requisito. Matriz de suporte versionada; nenhum login reinterpretado como API genérica.
- [ ] **M4-08** Extension host + package manager (fora do kernel): resolução de deps, lockfile reproduzível, instalação atômica, upgrade, rollback, revogação. Remoção não deixa capabilities/secrets/processos órfãos; upgrade incompatível bloqueado antes de alterar loadout.
- [ ] **M4-09** UX de permission review, trust tier e diagnóstico de compatibilidade — resumo humano de permissões na instalação.
- [ ] **M4-10** `Pack`/`Experience`/`Loadout`: setup guiado, defaults seguros, editor progressivo; usuário comum ativa experience sem conhecer crates/manifests. Packs não ganham authority própria; policy extension só restringe grants.
- [ ] **M4-11** Subagente = delegação limitada (objetivo, contexto derivado, capabilities, budget, prazo). Agente coletivo = owner/grupo, participantes, memória privada vs. compartilhada, identidade do solicitante, credenciais coletivas, conflict policy.
- [ ] **M4-12** Pack multi-extension de referência = **pack do uso real do owner** (decisão #13): Life OS/Developer pack (ex.: AgentRuntime coding + triggers de repo + goals/painel), validado por dogfooding diário, provando instalação→permission review→Loadout→execução→upgrade→rollback.
- [ ] **M4-13** Discovery híbrido (decisão #12): skills continuam no trilho agentskills.io (trust tier `community` + permission review); extensions/packs ganham catálogo próprio — índice git/estático oficial com assinatura de publisher e trust tiers. Marketplace rico só se comunidade aparecer.
- [ ] **M4-14** Contrato cloud-ready: daemon API/eventos, health/readiness, lifecycle, volume persistente, secrets por referência, import/export, hook de auth, container reproduzível, UI embutida idêntica local/hosted. **Sem control plane neste marco.**
- [ ] **M4-15** UI de extensões isolada por capability/sandbox; proibir execução arbitrária same-origin.

Gate: instalação nova funciona ponta a ponta; Agent atualiza em cadência própria sem mudança no kernel.

## M5. Segundo consumidor (host embedded)

Prova que o boundary não foi desenhado só para o Agent. Formato: **spike promovível** (decisão #9) — escrito com qualidade de produção, sem código descartável de propósito; vira fundação real do host fechado se a API sobreviver.

Slice: host embedded fechado injeta contexto autoritativo, registra action nomeada, policy externa autoriza/nega, commit ocorre fora do Bastion, OTel correlaciona turn+objeto.

- [ ] **M5-01** `AgentDefinition` owner-local criada fora do Bastion Agent; contexto via port público (sem patch no runtime); capability dinâmica object-scoped; policy fechada via adapter (sem fork do registry); session isolation por owner; trust/spotlighting/quarantine preservados; evento OTel neutro correlacionável sem trazer a timeline externa pro Core.
- [ ] **M5-02** Teste com dois workers / dois owners (revela assumptions pessoais ocultas).
- [ ] **M5-03** Assistente delega tarefa complexa sem compartilhar credencial pessoal; credencial pessoal owner-scoped nunca vira credencial de outro worker.
- [ ] **M5-04** Worker executa mudança pequena de código via `AgentRuntime` com diff/artefatos auditáveis.
- [ ] **M5-05** Pack vertical de referência materializa deployment owner-local; segundo owner adota o mesmo ancestral sem compartilhar deployment, secret, memória ou override; replay/upgrade/rollback preservam ancestry e isolamento.
- [ ] **M5-06** Propagação de regras versionadas: `RuleBundle v1`→dois workers do owner (terceiro owner não recebe); v2 com `effective_from` sem troca mid-turn; trace registra `rule.version`; rollback auditado; worker offline recupera revision correta; regra crítica stale segue policy explícita. Zero rebuild/redeploy, zero cross-owner, zero dependência de o LLM "lembrar de buscar".

Gate: zero import do Agent; zero fork do substrato; nenhuma entidade de negócio externa persiste no session store; findings de API voltam pro M3 antes do split físico.

## M6. Split físico + limpeza geral

Pré-condições: M0–M5 completos; dois consumidores reais; API exercitada.

- [ ] **M6-01** Repo atual vira `bastion-core` (preserva histórico/stars); produto extraído para `bastion-agent` com binário público `bastion` (decisão #2).
- [ ] **M6-02** Publicação: crates no crates.io a partir daqui (decisão #3); versões fixadas nos consumidores (nunca `main` flutuante); CI cross-repo (Core testa consumidores de referência; Agent testa min/max suportados); janela de compatibilidade + processo de upgrade documentados.
- [ ] **M6-03** **Limpeza geral do repo** (decisão #17): remover symlink `.planning`/tooling GSD e refs em AGENTS.md/CLAUDE.md; varredura de arquivos/pastas mortos (docs archive v2, skills órfãs, configs sem uso) com aprovação item a item; histórico de planejamento preservado no repo privado de arquitetura.
- [ ] **M6-04** Decommission por evidência, não por calendário: código legado com warning até replacement passar conformance+live E2E; shims com janela; `TerminalAgentProvider` removido via A-09; monorepo pré-split arquivado com redirects. **Nenhuma deleção destrutiva sem aprovação no momento.**

Gate: releases independentes e reproduzíveis; alteração do Agent não força release do Core; docs públicas não confundem Core, Agent e consumidores externos.

## M7. Validação viva, competitiva e de teses

Herda os débitos live da v1.1 (descidos do M0 — decisão #14):

- [ ] **M7-01** UAT-01: validação ao vivo E2E — containers MCP up, recall sanitizer ≥70%, schedule-fire, loop fechado skill-writer, providers cloud free.
- [ ] **M7-02** SO-05 live-verify: Gemini `thought_signature` em tool-use E2E.
- [ ] **M7-03** ≥1 canal externo (WhatsApp/Discord/Slack/Email) ponta-a-ponta ao vivo.
- [ ] **M7-04** FLUT-01: companion Flutter — pair (OTC) + SSE + cockpit validados ao vivo (sobre a UX nova do M4).

Benchmark e teses:

- [ ] **M7-05** Benchmark reproduzível vs. Hermes/OpenClaw: prompt injection indireta; ação destrutiva sem/com aprovação; conteúdo de canal público; tool result não confiável; vazamento `local-only`→cloud; memória falsa corrigida; crença expirada; separação entre personas; recuperação pós-restart; export/import; custo e tamanho de contexto; instalação até primeiro valor; memória/startup/artefato; login assinatura vs API key; start/resume/steer/cancel de coding agent; tarefa longa em background com conversa ativa; fidelidade de approvals/diff/artefatos do harness; instalação/upgrade/rollback de extensão; ativação de pack; extensão maliciosa tentando capability/secret/egress/owner não concedido; subagente e agente coletivo preservando identidade/escopo.
- [ ] **M7-06** Experimentos de tese com métricas de decisão (mesma `AgentDefinition` servindo agente pessoal e worker; consolidação com promoção governada; federação de aprendizado sem dado privado no artefato; packs reduzindo tempo de implantação; Cabinet A/B single-agent vs deliberation; assistente com escopos privado/time/grupo; timeline polimórfica `human|agent|system`).
- [ ] **M7-07** Publicar resultados internamente, **inclusive negativos**; nada vira pilar sem evidência; roadmap seguinte priorizado por métrica, não paridade de features.

---

## Invariantes — nunca regredir

- uma única superfície de invocação de capability (`CapabilityRegistry::invoke`);
- tool nomeada, nunca SQL cru;
- egress fail-closed por bloco/dado (`privacy_tier`);
- approval tipado, impossível de bypassar por caller ou adapter;
- trust acompanha tool result; conteúdo não confiável não recebe authority;
- isolamento por owner e sessão;
- contexto externo é opaco ao kernel;
- observabilidade vendor-neutral;
- host, não DAG/orquestrador;
- estado de negócio autoritativo permanece fora do Bastion.

## Gates contínuos

`cargo fmt --check` · `clippy --all-targets --all-features -D warnings` · `cargo test --workspace` · `#![forbid(unsafe_code)]` no kernel · API-breaking check · auditoria deps/licenças · matriz de feature flags · build mínimo sem produto · CI de dependências proibidas · diff de spans/event schema · limites de binário/startup/memória com tolerância registrada.

---

## Decisões (Q&A 2026-07-13 — substitui a antiga seção ABERTOS)

| # | Tema | Decisão |
|---|---|---|
| 1 | Granularidade | Intermediária: 10 crates + app (tabela em M1) |
| 2 | Nomes | Repo atual vira `bastion-core`; produto `bastion-agent`, binário `bastion` |
| 3 | Publicação | Git deps version-pinned na incubação; crates.io no M6 |
| 4 | Adapters | Providers concretos no substrato (`bastion-providers`, feature-gated); channels concretos no produto |
| 5 | `AgentDefinition` | `bastion-personas 0.x`; promove a estável quando o 2º consumidor provar |
| 6 | Cabinet | **Implementação compartilhada estável** no OSS (trait `DeliberationStrategy` + Cabinet como contrato) |
| 7 | Cognição | `bastion-memory` separada (quase-kernel); cognition+learning juntas |
| 8 | Migração v1.1 | **Nenhum migrator** — migração manual one-shot da instalação do owner; repo final o mais limpo possível |
| 9 | Slice M5 | Spike promovível (qualidade de produção, vira fundação se API sobreviver) |
| 10 | Licença | Source-available restritiva em todas as crates |
| 11 | Extension mechanisms | Declarativo + WASM/WASI + subprocess, **os três no primeiro release** |
| 12 | Registry | Híbrido: agentskills.io pra skills; catálogo próprio (índice git/estático) pra extensions/packs |
| 13 | Pack referência | Pack do uso real do owner (Life OS/Developer), dogfooding diário |
| 14 | M0 | Mínimo: tag + gates + métricas; validação live desce pra M7; UAT-02 absorvido por A-08 |
| 15 | Ordem | A-01/A-02 paralelos a M1; adapters após M2 |
| 16 | Versão | v2.0 |
| 17 | Limpeza/GSD | Limpeza geral (GSD, .planning, arquivos mortos) acontece no M6 |
| 18 | Scrub | Scrub corporativo total continua no material público |
