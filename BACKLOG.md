# Bastion Revamp — Backlog

> Reorganização do Bastion em **substrato reutilizável** (família de crates) + **produto pessoal** (Bastion Agent), com runtime de agentes externos (`AgentRuntime`) substituindo o terminal-agent legado, protocolo de extensões, backends por assinatura e split físico de repositórios ao final.
>
> Regra de ouro: **não reescrever enquanto separa**. Comportamento preservado primeiro, redesign depois. Nenhuma extração física antes do boundary provado no workspace.
>
> Convenções: `[ ]` pendente · `[~]` em progresso · `[x]` feito · itens marcados **(discutir)** dependem de decisão em aberto (seção final).

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

- [ ] **A-01** Contrato `AgentRuntime` + threat model: `start`/`resume`, `run_task`, eventos tipados, `steer`, `cancel`, status, timeout, workspace, sandbox, permission profile, approvals, usage, artefatos, correlação OTel.
- [ ] **A-02** Conformance suite comum (antes dos adapters): start/resume/steer/cancel/timeout/queue/streaming/diff+artefatos/permission profile/restart+crash recovery/OTel/auth-profile ref + declaração honesta de quais ações passaram pelo registry vs. ocorreram no sandbox externo. Teste negativo: rejeita stdout humano/ANSI e adapter incompatível; falha do client não corrompe sessão Bastion.
- [ ] **A-03** `CodexAppServerRuntime` nativo (Codex app-server): lifecycle, auth existente, sessões, eventos, steer/cancel, usage, artifacts.
- [ ] **A-04** `AcpxAgentRuntime`: processo `acpx` supervisionado, JSON-RPC/NDJSON `json-strict`, version pin, health/doctor. Tipos/paths/lifecycle do acpx não vazam pra API pública; cliente ACP Rust pode substituí-lo depois sem mudar `BackendProfile`.
- [ ] **A-05** Mesma conformance nos dois adapters + Codex nativo vs. Codex-via-ACP (prova que o contrato não foi moldado a uma implementação).
- [ ] **A-06** Runtime-backed **primary conversation**: Codex, Claude Code e OpenCode autenticados no host como `conversation_backend` de todos os turns.
- [ ] **A-07** Runtime-backed **delegated task**: `task_runtime` com conversa concorrente, cancelamento, retomada e restart recovery.
- [ ] **A-08** Security/live E2E + matriz versionada de targets, capabilities, auth e policy coverage.
- [ ] **A-09** Terminal-agent → feature `legacy-terminal-agent` → remoção após 1 release de deprecation (gate: A-05 + A-08 verdes, rollback legado testado).

## M0. Baseline congelada

Começar o revamp sobre uma v1.1 validada e reproduzível. Herda o Backlog descoped da Phase 12:

- [ ] **M0-01** UAT-01: validação ao vivo E2E — containers MCP up, recall sanitizer ≥70%, schedule-fire, loop fechado skill-writer, providers cloud free.
- [ ] **M0-02** UAT-02: terminal-agent E2E ao vivo **(discutir — pode ser absorvido pela Trilha A em vez de validar o legado)**.
- [ ] **M0-03** FLUT-01: companion Flutter — pair (OTC) + SSE + cockpit validados ao vivo.
- [ ] **M0-04** SO-05 live-verify: Gemini `thought_signature` em tool-use E2E.
- [ ] **M0-05** ≥1 canal externo (WhatsApp/Discord/Slack/Email) ponta-a-ponta ao vivo.
- [ ] **M0-06** Tag imutável `v1.1.0-pre-revamp` + métricas baseline registradas: testes, tamanho de binário, startup, memória idle, tempo de turn, superfícies públicas.
- [ ] **M0-07** Gate: checkout limpo; `cargo fmt --check` / `clippy -D warnings` / `cargo test` verdes; nenhum finding crítico aberto.
- [ ] **M0-08** Inventário de legado: classificar tudo como `keep|move|shim|delete-later` (alimenta M6).

## M1. Boundaries antes de mover código

- [ ] **M1-01** ADR "substrate, cognition and product split".
- [ ] **M1-02** Inventário módulo→destino confirmado por análise de dependências (hipótese inicial abaixo).
- [ ] **M1-03** Grafo de dependências real + ciclos que impedem separação.
- [ ] **M1-04** Lista de APIs públicas mínimas a estabilizar: `Runtime::run_turn(TurnRequest) -> TurnResult`; `Capability`/`CapabilityRegistry`/`InvokeContext`; `ContextProvider`/`ContextBlock`; `SessionStore`; `Provider`; `Observer`/event contract; ports opcionais de approval/budget/policy; `AgentDefinition` + bindings; `Memory`/`Belief`/proveniência; learning delta + interop; `ExtensionManifest`/`PackManifest` + lifecycle + permissões; `Loadout` resolvido + lockfile; delegação de subagente + ownership de agente coletivo; `AgentRuntime`; `AuthProfileRef`; `VersionedContextArtifact`/`ContextRevision`.
- [ ] **M1-05** Matriz mechanism/policy: o que é mecanismo OSS compartilhado vs. política do Agent vs. política de host externo.
- [ ] **M1-06** Política de estabilidade por crate (kernel semver estrito; cognição `0.x`; produto sem promessa de lib).
- [ ] **M1-07** Testes de caracterização das invariantes de policy boundary — escritos **antes** de mover qualquer código sensível.

Hipótese módulo→destino (confirmar em M1-02):

| Módulo atual | Destino | Nota |
|---|---|---|
| `agent/loop_`, `agent/handle` | `bastion-runtime` | loop, cancelamento, turn API |
| `types` | `bastion-types` | pequeno, sem deps de alto nível |
| `capability/*` | `bastion-capabilities` | policy boundary; approval via port |
| `agent/context` | `bastion-context` | blocos opacos + egress metadata |
| `session/*` | `bastion-sessions` | trait + SQLite default |
| `hooks/*` | kernel ou capabilities | separar hook genérico de policy concreta |
| `provider/*` | trait no kernel; concretos em adapter/Agent | |
| `mcp/*` | `bastion-mcp` | client/server como extensão oficial |
| `otel/*` | `bastion-observability` | convenções neutras, sinks plugáveis |
| `memory/*` | `bastion-memory` | beliefs, temporalidade, contestação |
| `agent/dream`, `agent/procedural`, `learn/*` | `bastion-cognition`/`bastion-learning` | fora do hot loop |
| `persona/*` | `bastion-personas` | `AgentDefinition` compartilhada; defaults pessoais saem |
| `cabinet/*` | `bastion-cognition` | `DeliberationStrategy` opcional |
| `goal/*`, `proactive/*` | `bastion-cognition` | primitives; policies fora |
| `mesh/*`, `identity/*`, `interop/*` | `bastion-mesh` | transporte neutro |
| `channel/*` | trait no runtime; transports no produto | |
| `api/*`, daemon `main` | `bastion-agent` | composição do produto |
| `provider/terminal_agent` | `bastion-agent-runtime` (legado até A-09) | |
| auth/keys/credentials | `bastion-auth` | referências opacas; secrets fora da definição |

## M2. Separação lógica no workspace

- [ ] **M2-01** Criar crates finos no workspace mantendo o binário atual funcionando.
- [ ] **M2-02** Extrair tipos folha sem lógica (types, mensagens, IDs, erros).
- [ ] **M2-03** Extrair capabilities/context/observability.
- [ ] **M2-04** Extrair sessions + provider traits.
- [ ] **M2-05** Extrair runtime/tool loop.
- [ ] **M2-06** Extrair memory/personas/cognition/learning/mesh como extensões.
- [ ] **M2-07** Binário atual vira composição das crates.
- [ ] **M2-08** CI de dependências proibidas (regras da seção 0).

Regras de migração: commits pequenos por boundary; comportamento preservado antes de redesign; re-exports temporários com data de remoção; zero rename cosmético misturado à extração; medir binário/performance a cada marco.

Gate: workspace compila por crate e como produto; kernel compila sem features de Agent; sem ciclos kernel↔cognition↔app; testes de caracterização da baseline verdes.

## M3. Substrato como biblioteca

- [ ] **M3-01** Reduzir `pub` ao contrato; erros tipados fail-closed nas bordas.
- [ ] **M3-02** Documentar invariantes de segurança: toda invocation passa pelo registry; `privacy_tier` ausente não vira allow; conteúdo não confiável não ganha autoridade; approval não bypassável por adapter; sessions owner-scoped; agente nunca recebe SQL cru.
- [ ] **M3-03** Compatibility tests contra a versão anterior suportada; checagem de API breaking no CI.
- [ ] **M3-04** Exemplos `minimal-agent` e `embedded-host` sem dependência do produto.
- [ ] **M3-05** Feature flags + matriz de combinações suportadas; build mínimo sem features de produto.
- [ ] **M3-06** Política de semver, MSRV, depreciação; docs de embedding/upgrade.
- [ ] **M3-07** `bastion-extension-protocol` + SDK. Mecanismos (nunca ABI dinâmica Rust como padrão comunitário): (1) artefatos declarativos; (2) WASM/WASI; (3) processo externo com protocolo versionado; (4) crate estática para extensão oficial/host embedded.
- [ ] **M3-08** Conformance de extensão: handshake, lifecycle, eventos, health, permissions, secrets, shutdown. Extensão de referência passa sem acesso implícito a processo/secrets/filesystem.
- [ ] **M3-09** Protótipos WASM/WASI + subprocess contra o mesmo modelo de capabilities.
- [ ] **M3-10** `ExtensionManifest`/`PackManifest` verificáveis: publisher/id, versão, compatibilidade, provides/requires, permissões, egress, devices, secrets, entrypoint, migrations, policy coverage. Assinatura + trust tier `official|verified|community|local`.
- [ ] **M3-11** Conformance de auth: login/refresh/revogação/quarantine/owner scope. Refresh tokens fora de config, log, export, prompt e definição de agente.
- [ ] **M3-12** `ContextRevision`: atualização só no boundary entre turns; estratégia explícita para revision stale (última válida ou fail-closed).

Gate: programa externo constrói e executa um turn só pela API documentada; security tests falham ao introduzir bypass conhecido; host externo implementa o protocolo de extensão sem ABI Rust.

## M4. Bastion Agent como produto

- [ ] **M4-01** App `bastion-agent` no workspace: daemon, canais concretos, config, installer, mobile, UX.
- [ ] **M4-02** `PersonalAgentPolicy` (memória, approval, routing, Dream, goals, proatividade).
- [ ] **M4-03** Migração automática config/DB v1.1 — versionada, idempotente, backup antes de mutar, rollback na janela; fixtures reais anonimizadas migram e reabrem.
- [ ] **M4-04** Posicionamento público: *agente pessoal longitudinal, contestável e authority-safe*.
- [ ] **M4-05** UX de diferenciação: fonte/validade de memória; correção/revogação; approvals pendentes; confiança/origem de conteúdo; local/cloud por tier; exportação/portabilidade.
- [ ] **M4-06** `BackendProfile`: `conversation_backend: ModelProvider|AgentRuntime`, `task_runtime: AgentRuntime?`, auth por backend, model/runtime id, permission+workspace policy, policy-coverage declaration por modo. UI distingue "Bastion tool loop" vs "harness tool loop".
- [ ] **M4-07** Login por assinatura como backend de primeira classe: Codex/ChatGPT (app-server), Claude (Claude Code/Agent SDK), Cursor (ACP), OpenCode (ACP + providers próprios). Instalação pessoal funciona **sem API key** quando há assinatura suportada; API tradicional continua suportada, nunca requisito. Matriz de suporte versionada; nenhum login reinterpretado como API genérica.
- [ ] **M4-08** Extension host + package manager (fora do kernel): resolução de deps, lockfile reproduzível, instalação atômica, upgrade, rollback, revogação. Remoção não deixa capabilities/secrets/processos órfãos; upgrade incompatível bloqueado antes de alterar loadout.
- [ ] **M4-09** UX de permission review, trust tier e diagnóstico de compatibilidade — resumo humano de permissões na instalação.
- [ ] **M4-10** `Pack`/`Experience`/`Loadout`: setup guiado, defaults seguros, editor progressivo; usuário comum ativa experience sem conhecer crates/manifests. Packs não ganham authority própria; policy extension só restringe grants.
- [ ] **M4-11** Subagente = delegação limitada (objetivo, contexto derivado, capabilities, budget, prazo). Agente coletivo = owner/grupo, participantes, memória privada vs. compartilhada, identidade do solicitante, credenciais coletivas, conflict policy.
- [ ] **M4-12** Pack multi-extension de referência (candidato: VTuber Pack demonstrativo — channel streaming, eventos, STT/TTS, avatar/OSC, persona, viewer memory, moderação, triggers, painel) provando instalação→permission review→Loadout→execução→upgrade→rollback.
- [ ] **M4-13** Contrato cloud-ready: daemon API/eventos, health/readiness, lifecycle, volume persistente, secrets por referência, import/export, hook de auth, container reproduzível, UI embutida idêntica local/hosted. **Sem control plane neste marco.**
- [ ] **M4-14** UI de extensões isolada por capability/sandbox; proibir execução arbitrária same-origin.

Gate: instalação nova e upgrade v1.1→Agent sem perda de dados; Agent atualiza em cadência própria sem mudança no kernel.

## M5. Segundo consumidor (host embedded)

Prova que o boundary não foi desenhado só para o Agent. Slice: host embedded fechado injeta contexto autoritativo, registra action nomeada, policy externa autoriza/nega, commit ocorre fora do Bastion, OTel correlaciona turn+objeto.

- [ ] **M5-01** `AgentDefinition` owner-local criada fora do Bastion Agent; contexto via port público (sem patch no runtime); capability dinâmica object-scoped; policy fechada via adapter (sem fork do registry); session isolation por owner; trust/spotlighting/quarantine preservados; evento OTel neutro correlacionável sem trazer a timeline externa pro Core.
- [ ] **M5-02** Teste com dois workers / dois owners (revela assumptions pessoais ocultas).
- [ ] **M5-03** Assistente delega tarefa complexa sem compartilhar credencial pessoal; credencial pessoal owner-scoped nunca vira credencial de outro worker.
- [ ] **M5-04** Worker executa mudança pequena de código via `AgentRuntime` com diff/artefatos auditáveis.
- [ ] **M5-05** Pack vertical de referência materializa deployment owner-local; segundo owner adota o mesmo ancestral sem compartilhar deployment, secret, memória ou override; replay/upgrade/rollback preservam ancestry e isolamento.
- [ ] **M5-06** Propagação de regras versionadas: `RuleBundle v1`→dois workers do owner (terceiro owner não recebe); v2 com `effective_from` sem troca mid-turn; trace registra `rule.version`; rollback auditado; worker offline recupera revision correta; regra crítica stale segue policy explícita. Zero rebuild/redeploy, zero cross-owner, zero dependência de o LLM "lembrar de buscar".

Gate: zero import do Agent; zero fork do substrato; nenhuma entidade de negócio externa persiste no session store; findings de API voltam pro M3 antes do split físico.

## M6. Split físico de repositórios

Pré-condições: M0–M5 completos; dois consumidores reais; API exercitada.

- [ ] **M6-01** Criar `bastion-core` e `bastion-agent` preservando histórico; binário público `bastion` sai do repo do Agent.
- [ ] **M6-02** Versões fixadas (nunca `main` flutuante); CI cross-repo (Core testa consumidores de referência; Agent testa min/max suportados); janela de compatibilidade + processo de upgrade documentados.
- [ ] **M6-03** Decommission por evidência, não por calendário: código legado com warning até replacement passar conformance+live E2E; shims com janela; docs antigas arquivadas; `TerminalAgentProvider` removido via A-09; monorepo pré-split arquivado com redirects. **Nenhuma deleção destrutiva sem aprovação no momento.**

Gate: releases independentes e reproduzíveis; alteração do Agent não força release do Core; docs públicas não confundem Core, Agent e consumidores externos.

## M7. Validação competitiva e de teses

- [ ] **M7-01** Benchmark reproduzível vs. Hermes/OpenClaw: prompt injection indireta; ação destrutiva sem/com aprovação; conteúdo de canal público; tool result não confiável; vazamento `local-only`→cloud; memória falsa corrigida; crença expirada; separação entre personas; recuperação pós-restart; export/import; custo e tamanho de contexto; instalação até primeiro valor; memória/startup/artefato; login assinatura vs API key; start/resume/steer/cancel de coding agent; tarefa longa em background com conversa ativa; fidelidade de approvals/diff/artefatos do harness; instalação/upgrade/rollback de extensão; ativação de pack; extensão maliciosa tentando capability/secret/egress/owner não concedido; subagente e agente coletivo preservando identidade/escopo.
- [ ] **M7-02** Experimentos de tese com métricas de decisão (mesma `AgentDefinition` servindo agente pessoal e worker; consolidação com promoção governada; federação de aprendizado sem dado privado no artefato; packs reduzindo tempo de implantação; Cabinet A/B single-agent vs deliberation; assistente com escopos privado/time/grupo; timeline polimórfica `human|agent|system`).
- [ ] **M7-03** Publicar resultados internamente, **inclusive negativos**; nada vira pilar sem evidência; roadmap seguinte priorizado por métrica, não paridade de features.

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

`cargo fmt --check` · `clippy --all-targets --all-features -D warnings` · `cargo test --workspace` · `#![forbid(unsafe_code)]` no kernel · API-breaking check · auditoria deps/licenças · matriz de feature flags · build mínimo sem produto · CI de dependências proibidas · testes de migração DB/config · diff de spans/event schema · limites de binário/startup/memória com tolerância registrada.

---

## ABERTOS — discutir antes de executar

1. **Granularidade de crates**: 4–6 crates maiores vs. topologia detalhada (~15). Recomendação do plano: começar coarse, dividir por pressão real.
2. **Nomes**: repo/lib `bastion-core` vs `bastion-sdk` vs `bastion`; produto `bastion-agent` no Git com binário `bastion`?
3. **Publicação**: crates.io desde já vs. registry Git durante incubação.
4. **Providers/channels concretos**: adapters oficiais no repo do substrato vs. só no produto.
5. **`AgentDefinition`**: crate estável imediata vs. amadurecer em `bastion-personas 0.x`.
6. **Cabinet**: implementação compartilhada vs. só o trait `DeliberationStrategy` no OSS.
7. **Boundary fino** entre `bastion-memory` / `bastion-cognition` / `bastion-learning`.
8. **Janela de suporte** à config/DB v1.1 (quanto tempo o migrator vive).
9. **Consumidor do slice M5**: spike descartável vs. primeira fundação real do host.
10. **Licença** das extensões cognitivas (a fronteira "OPEN = Bastion" precisa ser explicitada por crate).
11. **Boundary inicial** declarativo vs WASM/WASI vs subprocess vs crate oficial — o que entra no primeiro release.
12. **Registry de extensões**: federado vs. catálogo central; publisher verification.
13. **VTuber Pack**: slice público inicial vs. fixture de conformance.
14. **M0 — quanto executar de verdade**: UAT-02 valida o terminal-agent legado que a Trilha A vai matar — validar mesmo assim ou absorver? FLUT-01 bloqueia baseline ou desce pra M4?
15. **Ordem/paralelismo**: Trilha A começa junto com M1–M2 ou só depois do workspace extraído? (A-01/A-02 são só contrato+testes — candidatos a paralelo imediato.)
16. **Versão/tag**: revamp vira v2.0? Afeta naming de tags, migrations e comunicação pública.
17. **Destino da tooling de planejamento atual** (.planning/GSD): deletar quando? O que preservar como histórico?
18. **Este arquivo é público**: manter scrub de nomes de consumidores fechados (regra atual) — qualquer detalhamento sensível fica no repo privado de arquitetura.
