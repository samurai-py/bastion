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
- [x] **A-03** `CodexAppServerRuntime` nativo (`02600ef`) — validado LIVE contra Codex logado (ChatGPT plan); JSON-RPC app-server (turn/start, turn/completed); resume=true, steer=true (retry contra race de readiness), approvals=Bridged, sandbox=Partial.
- [x] **A-04** `AcpxAgentRuntime` (`41e7557`) — validado LIVE via acpx→Claude Code local; NDJSON `--format json`, env_clear+allowlist, version pin, resume=NotResumable honesto; approvals=HarnessOwned declarado.
- [x] **A-05** Matriz em `docs/revamp/A-05-conformance-matrix.md` (`7c0e95a`): mesma suite nos dois (9 Pass cada; Skips documentados). *Codex-via-acpx indisponível (bridge só oferece modelo rejeitado pelo plano — HTTP 400); opencode requer `opencode auth login`. Completar essas duas células quando destravar.* 6 furos do contrato A-01 achados na prática (ver LOOP-REPORT #5).
- [x] **A-06** (Ciclo 2.4, `8b2cae4`) Runtime-backed **primary conversation** — `BackendProfile.conversation == Runtime(id)` desvia `run_turn_for_with_trust` inteiro pro harness (`AgentLoop::run_runtime_backed_turn`); validado LIVE via `AcpxAgentRuntime("claude")`→Claude Code, através do caminho real do daemon (`AgentLoop::run_turn_for`), resposta correta + memória gravada (`docs/revamp/A-06-A-07-live.md`). *Claude Code é o `conversation_backend` provado ao vivo; Codex prova o MESMO caminho de código (`AgentRuntime`) ao vivo em A-07 (como `task_runtime`, não `conversation_backend` — a diferença é só qual campo do `BackendProfile` aponta pro id, o código do turno é idêntico); OpenCode segue bloqueado por auth (mesmo furo do A-05).*
- [x] **A-07** (Ciclo 2.4, `c69a810`) Runtime-backed **delegated task** — `AgentLoop::delegate_task`/`cancel_delegated_task`/`resume_delegated_task`; validado LIVE contra `codex app-server`: delega (1.6s, não bloqueia), conversa concorrente no MESMO `AgentLoop` responde em 19ms, 2ª task cancelada ~2s depois reporta `Cancelled`, sessão morta (restart simulado) reatada via `resume_delegated_task` + `ResumeSpec` completa uma task de follow-up — tudo entregue via reuso do seam PROACT-05 (`pending_tx`). Achado de contrato (não defeito): `resume()` reata a SESSÃO, nenhum adapter faz replay de uma task já em voo através de uma reconexão perdida — `resume_delegated_task` é honesto sobre isso e submete uma NOVA task de follow-up. Placar completo em `docs/revamp/A-06-A-07-live.md`.
- [ ] **A-08** Security/live E2E + matriz versionada de targets, capabilities, auth e policy coverage. *(Absorve o antigo UAT-02: o legado não será validado, será substituído.)*
- [ ] **A-09** Terminal-agent → feature `legacy-terminal-agent` → remoção após 1 release de deprecation (gate: A-05 + A-08 verdes, rollback legado testado).

## M0. Baseline congelada (mínima)

Decisão: **sem validação ao vivo neste marco** — os débitos live herdados da v1.1 (UAT-01, FLUT-01, SO-05, canal externo) descem pra M7/uso real; UAT-02 morre absorvido pela Trilha A (A-08).

- [x] **M0-01** Tag imutável `v1.1.0-pre-revamp` (→ 1528759) + métricas em `docs/revamp/BASELINE.md` (525 testes, binário 24MB ⚠️>20MB, 33k LOC, 283 pub).
- [x] **M0-02** Gates verdes: fmt limpo, clippy exit 0 (future-incompat de dep anotado), 525 testes.
- [x] **M0-03** Inventário `keep|move|shim|delete-later` em `docs/revamp/LEGACY-INVENTORY.md` (3 itens `?` pra resolver antes do M6).

## M1. Boundaries antes de mover código

- [x] **M1-01** ADR em `docs/revamp/M1-ADR-substrate-split.md` (`4bfaef6`).
- [x] **M1-02** Inventário módulo→destino confirmado (ADR + execução M2 — desvios documentados: scheduler→mesh, terminal_agent→providers).
- [x] **M1-03** Grafo real medido: 27 arestas proibidas em 4 padrões (V1-V4), todas quebradas no M2; acíclico confirmado pelo CI.
- [x] **M1-04** Lista de APIs públicas mínimas a estabilizar (no ADR; estabilização efetiva = M3): `Runtime::run_turn(TurnRequest) -> TurnResult`; `Capability`/`CapabilityRegistry`/`InvokeContext`; `ContextProvider`/`ContextBlock`; `SessionStore`; `Provider`; `Observer`/event contract; ports opcionais de approval/budget/policy; `AgentDefinition` + bindings; `Memory`/`Belief`/proveniência; learning delta + interop; `ExtensionManifest`/`PackManifest` + lifecycle + permissões; `Loadout` resolvido + lockfile; delegação de subagente + ownership de agente coletivo; `AgentRuntime`; `AuthProfileRef`; `VersionedContextArtifact`/`ContextRevision`; `DeliberationStrategy` + Cabinet (contrato estável — decisão #6).
- [x] **M1-05** Matriz mechanism/policy (regra única no ADR: crates = mecanismo configurável, opinião = política injetada; teste real no M5).
- [x] **M1-06** Política de estabilidade por crate (tabela do ADR).
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
- [x] **M2-03/04/05** `bastion-runtime` extraída (`849e67d` + 3 commits de ports): capability/session/hooks/agent-core + traits Provider e Memory no kernel; 9 ports (Responder, TurnKernel, FailureSink, ToolSource, GoalPort, CommandHandler, PreCompactionFlush, ToolResultObserver, ProviderResolver); 535 testes/24 suites; binário +0,16%.
- [x] **M2-04b** `bastion-memory` extraída (`f6575b5`) — backend SqliteMemory implementa o trait do kernel; aresta V4 memory→mesh era test-only (testes relocados pro allowlist).
- [x] **M2-05b** `bastion-providers` (`9ed9844` — V4 ollama→cabinet cortado via CabinetVerdict→types; terminal_agent foi junto, divergência da tabela documentada), `bastion-mcp` (`0488259` — BastionMcpServer fica pro passo 6/7) e `bastion-agent-runtime` (`b614f01`) extraídas; 535 testes; binário +0,19% acumulado.
- [x] **M2-06** `bastion-cognition` (`b46c28f`), `bastion-personas` (`535c7cc`), `bastion-mesh` (`adb13c8` — scheduler/cron foi pra mesh, é sync de mesh puro) extraídas; cabinet→persona invertido via closure; tipos de router/persona puros pra bastion-types; zero ciclos; 535 testes/38 suites.
- [x] **M2-07** Binário atual vira composição das crates (root `Cargo.toml` depende das 9 crates; `src/` reduzido a app + 19 shims de re-export datados — auditoria e inventário em `docs/revamp/M2-CLOSE.md`).
- [x] **M2-08** CI de dependências proibidas: `scripts/check-crate-deps.sh` valida allowlist exata por crate + zero ciclos + nenhuma crate → pacote raiz `bastion`; validado contra o estado atual (PASS, zero discrepâncias); job `crate-deps` em `.github/workflows/ci.yml` roda antes do job `rust` (fmt/clippy/test).

Regras de migração: commits pequenos por boundary; comportamento preservado antes de redesign; re-exports temporários com data de remoção; zero rename cosmético misturado à extração; medir binário/performance a cada marco.

Gate: workspace compila por crate e como produto; kernel compila sem features de Agent; sem ciclos kernel↔cognition↔app; testes de caracterização da baseline verdes.

## M3. Substrato como biblioteca

Distribuição durante incubação: **path deps no workspace + git deps version-pinned para consumidores externos; crates.io só no M6** (decisão #3).

- [ ] **M3-01** Reduzir `pub` ao contrato; erros tipados fail-closed nas bordas.
- [x] **M3-02** Documentar invariantes de segurança: toda invocation passa pelo registry; `privacy_tier` ausente não vira allow; conteúdo não confiável não ganha autoridade; approval não bypassável por adapter; sessions owner-scoped; agente nunca recebe SQL cru. (`docs/SECURITY-INVARIANTS.md`)
- [ ] **M3-03** Compatibility tests contra a versão anterior suportada; checagem de API breaking no CI.
- [x] **M3-04** Exemplos `minimal-agent` e `embedded-host` sem dependência do produto. (3 furos de API achados — `docs/revamp/M3-CLOSE.md` §3)
- [x] **M3-05** Feature flags + matriz de combinações suportadas; build mínimo sem features de produto. (`channels-extra`/`voice`/`mcp-server`; mínimo 15,6 MB vs full 24,3 MB; flag `mesh` pulada — webhook refactor, `docs/revamp/M3-CLOSE.md` §4)
- [ ] **M3-06** Política de semver, MSRV, depreciação; docs de embedding/upgrade. Licença: **source-available restritiva em todas as crates** (decisão #10).
- [ ] **M3-07** `bastion-extension-protocol` + SDK — **os três mecanismos no primeiro release** (decisão #11): (1) artefatos declarativos; (2) WASM/WASI; (3) processo externo com protocolo versionado. Crate estática permanece caminho para extensão oficial/host embedded. Nunca ABI dinâmica Rust como padrão comunitário.
- [ ] **M3-08** Conformance de extensão nos três mecanismos: handshake, lifecycle, eventos, health, permissions, secrets, shutdown. Extensão de referência passa sem acesso implícito a processo/secrets/filesystem.
- [ ] **M3-09** `ExtensionManifest`/`PackManifest` verificáveis: publisher/id, versão, compatibilidade, provides/requires, permissões, egress, devices, secrets, entrypoint, migrations, policy coverage. Assinatura + trust tier `official|verified|community|local`.
- [ ] **M3-10** Conformance de auth: login/refresh/revogação/quarantine/owner scope. Refresh tokens fora de config, log, export, prompt e definição de agente.
- [x] **M3-11** `ContextRevision`: atualização só no boundary entre turns; estratégia explícita para revision stale (última válida ou fail-closed). **Entregue no Loop 3-E** junto com M5-06: `VersionedContextArtifact`/`ContextRevision`/`StalePolicy` nasceram em `bastion-types` (`crates/bastion-types/src/context_artifact.rs`) — o contrato não existia antes (era só uma linha na lista de APIs públicas mínimas do M1-ADR). `effective_at(now)` nunca troca dentro de um turn já resolvido (só entre turns); `StalePolicy::UseLastKnown`/`FailClosed` é campo do artefato, a DETECÇÃO de staleness fica a cargo do host (ver rustdoc do módulo pra por quê).

Gate: programa externo constrói e executa um turn só pela API documentada; security tests falham ao introduzir bypass conhecido; host externo implementa o protocolo de extensão sem ABI Rust.

## M4. Bastion Agent como produto

- [ ] **M4-01** App `bastion-agent` no workspace: daemon, canais concretos, config, installer, mobile, UX.
- [ ] **M4-02** `PersonalAgentPolicy` (memória, approval, routing, Dream, goals, proatividade).
- [ ] **M4-03** **Sem migração automática** (decisão #8): instalação v1.1 do owner é migrada manualmente uma vez; nenhum código de migrator entra no produto. Export/import `.af` continua sendo o caminho de portabilidade.
- [ ] **M4-04** Posicionamento público: *agente pessoal longitudinal, contestável e authority-safe*.
- [ ] **M4-05** UX de diferenciação: fonte/validade de memória; correção/revogação; approvals pendentes; confiança/origem de conteúdo; local/cloud por tier; exportação/portabilidade.
- [~] **M4-06** `BackendProfile`: `conversation_backend: ModelProvider|AgentRuntime`, `task_runtime: AgentRuntime?`, auth por backend, model/runtime id, permission+workspace policy, policy-coverage declaration por modo. UI distingue "Bastion tool loop" vs "harness tool loop". **Kernel wiring entregue no Ciclo 2.4** (`docs/revamp/C2-backend-profile-design.md`, A-06/A-07 acima): `ConversationBackend`/`BackendProfile`/`RuntimeRegistry` em `bastion-runtime`, `[backend]` TOML declarativo, `coverage_note` populado do `RuntimeDescriptor`. **UX de seleção e matriz de assinatura versionada entregues no Loop 3-B** (ver M4-07 abaixo). Falta pro M4 pleno: login guiado/OAuth interativo (Bastion não realiza login algum, só verifica um login de CLI já feito pelo owner — fronteira deliberada de segurança de credencial, não um corte de escopo acidental), `task_runtime` como tool exposta ao modelo com policy fina (design doc §6).
- [~] **M4-07** Login por assinatura como backend de primeira classe: Codex/ChatGPT (app-server), Claude (Claude Code/Agent SDK), Cursor (ACP), OpenCode (ACP + providers próprios). Instalação pessoal funciona **sem API key** quando há assinatura suportada; API tradicional continua suportada, nunca requisito. Matriz de suporte versionada; nenhum login reinterpretado como API genérica. **Loop 3-B (`fix(c3)`/`feat(c3)` commits, 2026-07-14):** opencode-via-acpx desbloqueado (`--auth-policy` configurável por agente — era o bloqueio §2A da A-05); `AuthResolver` (porta no kernel, `bastion-runtime/agent/ports.rs`) + `AuthProfileRegistry` (app, `src/auth_profile_registry.rs`) resolvem `AuthProfileRef` de verdade por referência (spawna o próprio "whoami" read-only de cada CLI — `claude auth status`/`codex login status`/`opencode auth list` — nunca lê/loga token); critério de aceite provado AO VIVO com zero `*_API_KEY` no ambiente (`tests/agent_runtime_backend_live.rs::m4_07_subscription_backend_works_without_api_key_live`); UX de seleção (`/backends`, `/backend use ...`, cockpit command, sem GUI) + `docs/SUPPORT-MATRIX.md` (público, versionado, derivado dos descriptors reais). **Ainda falta:** adapter Cursor (ACP) não existe; `codex` via `acpx` segue indisponível (A-05 §4, login-mode mismatch externo aos adapters).
- [x] **M4-08** Extension host + package manager (fora do kernel): resolução de deps, lockfile reproduzível, instalação atômica, upgrade, rollback, revogação. Remoção não deixa capabilities/secrets/processos órfãos; upgrade incompatível bloqueado antes de alterar loadout. **Entregue no Loop 3-C** (`docs/revamp/C3-extension-protocol-design.md`): `bastion-extension-protocol` crate (contratos: `ExtensionManifest`/`PackManifest`/`PermissionSet`/`ExtensionError`/`LoadoutLock`) + `src/extension/host.rs`'s `ExtensionHost` (app, fora do kernel). `install`/`upgrade`/`revoke` são atômicos (confiam só no que `HostFacade::registered_capabilities()` reporta ter registrado de verdade, nunca no que o manifest afirma, pra rollback exato); `upgrade` bloqueia ANTES de tocar o loadout ativo tanto por range de compat do protocolo quanto por `requires` de um dependente instalado — testado em `tests/extension_adversarial.rs`. Zero órfão provado em `tests/extension_reference_pack.rs` (revoke de um pack de 3 extensões deixa registry e lockfile vazios).
- [~] **M4-09** UX de permission review, trust tier e diagnóstico de compatibilidade — resumo humano de permissões na instalação. **Núcleo entregue no Loop 3-C:** `src/extension/review.rs::permission_summary`/`trust_tier_of` — texto owner-facing 1:1 com o que `PermissionSet`/`HostFacade` de fato aplicam, exercitado no fluxo de review do pack de referência ANTES do install. `trust_tier_of` é deliberadamente conservador (sempre `Local` — verificação real de assinatura publisher fica pro M4-13). **Falta pro pleno:** superfície de UX de fato (comando cockpit tipo `/extensions install <id>` mostrando esse resumo interativamente) — a função de formatação existe e está testada, mas não está fiada num fluxo de produto ainda.
- [~] **M4-10** `Pack`/`Experience`/`Loadout`: setup guiado, defaults seguros, editor progressivo; usuário comum ativa experience sem conhecer crates/manifests. Packs não ganham authority própria; policy extension só restringe grants. **Mecânica entregue no Loop 3-C:** `ExtensionHost::resolve_pack`/`loadout()` (`LoadoutDefaults`) — um pack nunca amplia a autoridade de um membro além do que a instância concede (testado com um 4º membro hipotético mais ganancioso bloqueado ANTES de qualquer install, `tests/extension_reference_pack.rs`/`tests/extension_adversarial.rs`). **Falta pro pleno:** "editor progressivo"/setup guiado como superfície de produto (hoje é só a mecânica de resolução, sem UX).
- [ ] **M4-11** Subagente = delegação limitada (objetivo, contexto derivado, capabilities, budget, prazo). Agente coletivo = owner/grupo, participantes, memória privada vs. compartilhada, identidade do solicitante, credenciais coletivas, conflict policy. **Fora do escopo do Loop 3-C** (não fazia parte dos commits deste loop) — segue pendente.
- [x] **M4-12** Pack multi-extension de referência = **pack do uso real do owner** (decisão #13): Life OS/Developer pack (ex.: AgentRuntime coding + triggers de repo + goals/painel), validado por dogfooding diário, provando instalação→permission review→Loadout→execução→upgrade→rollback. **Entregue no Loop 3-C** (`tests/extension_reference_pack.rs`): pack `mario/life-os-developer-pack` compondo os 3 kinds implementados (Declarative: prompt diário de reflexão; Subprocess: trigger de repo via processo real; Wasm: cálculo de orçamento sandboxed) — ciclo completo install→review→Loadout resolvido→execução dos 3→upgrade→rollback→revoke, zero órfão verificado.
- [ ] **M4-13** Discovery híbrido (decisão #12): skills continuam no trilho agentskills.io (trust tier `community` + permission review); extensions/packs ganham catálogo próprio — índice git/estático oficial com assinatura de publisher e trust tiers. Marketplace rico só se comunidade aparecer. *Numeração discrepante achada no Loop 3-D: `docs/revamp/C3-cloud-ready-design.md` se autotitula "M4-13..15" mas seu conteúdo só cobre M4-14/M4-15 — este item (discovery híbrido) não é abordado em nenhuma seção do doc. Segue `[ ]`, não confundir com "coberto pelo design cloud-ready".*
- [x] **M4-14** Contrato cloud-ready: daemon API/eventos, health/readiness, lifecycle, volume persistente, secrets por referência, import/export, hook de auth, container reproduzível, UI embutida idêntica local/hosted. **Sem control plane neste marco.** **Entregue no Loop 3-D** (`docs/revamp/C3-cloud-ready-design.md`): `SecretRef`/`SecretResolver` (`bastion-types::secret` + `src/secret.rs` — env/mounted-file/layered, `APP_JWT_SECRET`/`BASTION_INFER_TOKEN` resolvidos por referência, gap do M4-08 fechado no `Subprocess` mechanism); `/healthz`+`/readyz` (`src/channel/operational.rs`, montados no mesmo axum router do webhook) com `ReadinessState` genuína (session/memory/provider prontos no início do `daemon_loop`, `channels` só após todo spawn terminar); `POST /lifecycle/stop`/`reload` gateados por `DaemonAccessAuth` (token por referência via `BASTION_DAEMON_TOKEN`); `.af` ganha `producer` (default-populated, zero breaking pra arquivo legado); volume/paths já eram env-overridable (`BASTION__SESSION__DB_PATH`/`BASTION__LOGGING__LOG_PATH`), agora com teste dedicado; `tests/boot_local_and_hosted.rs` prova o mesmo binário bootando local/hosted-like sem recompilar. **Achado reportado, não alterado:** `--with-identity` no `.af` continua embutindo o keypair age/Ed25519 em texto puro por design (identidade portável, não secret-por-referência) — documentado + testado como exceção deliberada, não gap silencioso. `docker build` real não executado (disco do sandbox, ver LOOP-REPORT) — Dockerfile revisado estaticamente, já parametrizado via `BASTION__*`/`BASTION_CONFIG`, sem path/secret hardcoded.
- [x] **M4-15** UI de extensões isolada por capability/sandbox; proibir execução arbitrária same-origin. **Entregue no Loop 3-D** (`docs/revamp/C3-cloud-ready-design.md` §Ponto de segurança 2): `ExtensionUiHost` (`src/extension/ui.rs`) serve os assets de `Provided::Ui` com `Content-Security-Policy: sandbox allow-scripts` (sem `allow-same-origin` — opaque-origin real pro browser) + `nosniff`/`frame-ancestors`; único canal de volta ao backend é `POST /{id}/invoke`, mediado pelo `CapabilityRegistry` e gateado pelo `PermissionSet` da própria extensão (capability fora do declarado → `ExtensionError::CapabilityNotDeclared` tipado, nunca alcança o registry real). Suíte adversarial nova (`tests/extension_ui_adversarial.rs`) prova os 2 vetores da spec. **Falta pro pleno:** nenhum consumidor de UI web real existe neste repo ainda pra wire — mecanismo pronto e testado, integração em `main.rs`/produto é passo de composição posterior (mesmo padrão M4-09/M4-10).

Gate: instalação nova funciona ponta a ponta; Agent atualiza em cadência própria sem mudança no kernel.

## M5. Segundo consumidor (host embedded)

Prova que o boundary não foi desenhado só para o Agent. Formato: **spike promovível** (decisão #9) — escrito com qualidade de produção, sem código descartável de propósito; vira fundação real do host fechado se a API sobreviver.

Slice: host embedded fechado injeta contexto autoritativo, registra action nomeada, policy externa autoriza/nega, commit ocorre fora do Bastion, OTel correlaciona turn+objeto.

- [x] **M5-01** `AgentDefinition` owner-local criada fora do Bastion Agent; contexto via port público (sem patch no runtime); capability dinâmica object-scoped; policy fechada via adapter (sem fork do registry); session isolation por owner; trust/spotlighting/quarantine preservados; evento OTel neutro correlacionável sem trazer a timeline externa pro Core. **Entregue no Loop 3-E** (`docs/revamp/C3-m5-second-consumer-design.md`, `examples/embedded-host-slice/`): os 7 componentes rodam contra API pública só (`bastion-types`/`bastion-runtime`/`bastion-memory`/`bastion-personas`), zero import de `bastion`, zero fork. **Achado, não silenciado:** o evento OTel do turn (`gen_ai.conversation.id` no span `invoke_agent`) é estampado do `AgentLoop.session_id` de CONSTRUÇÃO, ANTES da resolução de sessão por-owner (CR-04) — para qualquer owner ≠ o da construção, o atributo é sempre o mesmo valor errado; nenhum outro atributo do span identifica o owner. Correlação hoje só funciona por ORDEM de chamada, não por atributo — ver `docs/revamp/LOOP-REPORT.md` (Loop 3-E) pro detalhe e sugestão de fix (não aplicado, fora do escopo do slice — mudança de comportamento em arquivo de contrato estável).
- [x] **M5-02** Teste com dois workers / dois owners (revela assumptions pessoais ocultas). **Entregue no Loop 3-E**: um `AgentLoop`, dois owners (`owner_a`/`owner_b`), sessões CR-04 separadas provadas (ids diferentes, históricos sem cross-leak), contexto autoritativo e RuleBundle nunca cruzam owner.
- [ ] **M5-03** Assistente delega tarefa complexa sem compartilhar credencial pessoal; credencial pessoal owner-scoped nunca vira credencial de outro worker.
- [ ] **M5-04** Worker executa mudança pequena de código via `AgentRuntime` com diff/artefatos auditáveis.
- [ ] **M5-05** Pack vertical de referência materializa deployment owner-local; segundo owner adota o mesmo ancestral sem compartilhar deployment, secret, memória ou override; replay/upgrade/rollback preservam ancestry e isolamento.
- [x] **M5-06** Propagação de regras versionadas: `RuleBundle v1`→dois workers do owner (terceiro owner não recebe); v2 com `effective_from` sem troca mid-turn; trace registra `rule.version`; rollback auditado; worker offline recupera revision correta; regra crítica stale segue policy explícita. Zero rebuild/redeploy, zero cross-owner, zero dependência de o LLM "lembrar de buscar". **Entregue no Loop 3-E (chamado "M5.1" na spec do orquestrador — mesmo item, numeração divergente entre BACKLOG e design doc, sinalizado aqui em vez de resolvido por adivinhação):** o contrato `VersionedContextArtifact`/`ContextRevision` (já previsto em M1-ADR-substrate-split.md §"APIs públicas mínimas", mas NUNCA implementado antes deste loop — zero hits no grep pré-loop) nasceu em `bastion-types` (`crates/bastion-types/src/context_artifact.rs`) — artefato opaco versionado + provenance + `effective_from` + `StalePolicy` (UseLastKnown/FailClosed), conservador (só o tipo, nenhum port novo). Os 7 passos de propagação do design doc (a spec fala em "8 passos" nos critérios de aceite, mas a própria lista numerada tem 7 itens — mesma discrepância de contagem sinalizada, não inventado um 8º passo) todos provados em `examples/embedded-host-slice/src/rule_bundle.rs`.

Gate: zero import do Agent; zero fork do substrato; nenhuma entidade de negócio externa persiste no session store; findings de API voltam pro M3 antes do split físico. **M5-01/02/06 cumprem o gate** (verificado no Loop 3-E: `examples/embedded-host-slice` depende só de `bastion-*`, nenhuma entidade do host chega ao session store do Bastion — ver `docs/revamp/LOOP-REPORT.md`). M5-03/04/05 (delegação de tarefa, `AgentRuntime` code-change, pack vertical de referência) seguem fora do escopo deste loop — não pedidos, não abordados.

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
