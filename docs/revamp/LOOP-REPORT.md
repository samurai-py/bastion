# Loop autônomo — relatório de execução e findings

> Corrida iniciada 2026-07-13. Itens que exigem olho do owner ou follow-up marcados por seção.

## Findings de review (não bloqueantes, endereçados no fluxo)

1. ~~**[M3] `ToolSource::call_tool_with_timeout` é ungated por construção.**~~ **RESOLVIDO (M3-01).** Os dois call sites pré-existentes de bypass do registry (fallback de registry vazio em `dispatch_tool_loop` e o tool-loop inteiro de `run_provider_fallback`) aplicavam `check_egress` manualmente ANTES de chamar (WR-02/D-13). O trait `ToolSource::call_tool_with_timeout` (`crates/bastion-runtime/src/agent/ports.rs`) agora recebe `resolved_tier: Option<PrivacyTier>` e a implementação de produção (`McpToolSource`, `crates/bastion-mcp/src/tool_source.rs`) chama `check_egress` internamente ANTES de despachar — os dois call sites só passam o tier adiante, não chamam `check_egress` mais. Comportamento idêntico (mesma checagem, mesmo chokepoint lógico), agora inesquecível por construção. Coberto por teste de invariante novo (`tests/characterization_boundary.rs::tool_source_gate_blocks_dispatch_on_local_only_tier` + `::mcp_tool_source_gates_egress_before_attempting_dispatch`, mapeado em `docs/revamp/M1-07-characterization-map.md` linha "F1"). Trust-tagging paridade (o gap de `TaggedValue` nesses paths de bypass) permanece pré-existente e fora de escopo desta correção — não endereçado aqui.
2. **[3b] `GoalPort` retornava `crate::goal::Goal` na assinatura do kernel** — vazamento de tipo de cognição. Resolvido no 3b movendo `Goal` (e `PrivacyTier`) pra `bastion-types`.
3. **[M3→M4] Furos de API de approval descobertos pelo `embedded-host` (M3-CLOSE.md §3):** (a) `AgentLoop::new` hardwira `ApprovalQueue::new(db_path)` — host não injeta política própria; (b) `ApprovalQueue` é struct SQLite concreta, não port (M1-04 previa "ports opcionais de approval"); (c) rejeição invisível — `Rejected` mapeia pra `AlreadyPending`, re-invocação retorna `Ok({awaiting_approval:true})` em vez de Err tipado (assimetria com `PrivacyEgressBlocked`). O (c) é mudança de comportamento observável — desenhar com cuidado (Err tipado `ApprovalDenied`), executar antes do M5. O exemplo tem asserts que quebram quando isso for corrigido — atualizar junto.
4. **[pré-existente, M3+] Paridade de trust-tagging nos paths de bypass do ToolSource** — bypass egress-gated mas resultados não recebem `TaggedValue` untrusted como `registry.invoke` aplica. Avaliar junto do fix de approval.
5. **[A-05 → revisão do contrato A-01] 6 furos achados na validação live dos adapters** (detalhe em `docs/revamp/A-05-conformance-matrix.md` §5): (1) `WATCHDOG` 5s da conformance é apertado pra harness cloud frio — parametrizar; (2) sandbox degrada silenciosamente sem bubblewrap — `SandboxCoverage` precisa ser detectado, não declarado estático; (3) race de readiness no `turn/steer` do Codex (mitigado com retry bounded no adapter); (4) `turn/interrupt` ambíguo cancel/timeout (mitigado com tracking client-side); (5) **SEGURANÇA (T4-adjacente):** `respond_permission(Deny)` gateia UMA tool-call — modelo capaz contorna por tool alternativa não-gateada; deny precisa poder escalar pra cancel-turn ou deny-classe, decisão de design pro M4/BackendProfile; (6) `resume()` não recebe `SessionSpec` — política env/timeout/permission não é recuperável no reattach, adapter usa defaults conservadores; considerar `ResumeSpec` no contrato.

## Encerramento do loop (2026-07-14 ~00:15)

Entregue nesta corrida: M0, M1 completo, M2 completo (9 crates + app + CI deps), M3 estático (F1 hardening, invariantes doc, exemplos, feature flags, build mínimo 15,6MB), Trilha A até A-05 (contrato, conformance, 2 adapters com validação LIVE, matriz). 537 testes / 40 suites verdes; origin/revamp atualizado a cada marco.

**Fila do próximo ciclo (em ordem):** (1) fix dos furos de approval (finding #3) — pré-requisito do M5; (2) revisão do contrato A-01 com os 6 furos do finding #5; (3) M4 (BackendProfile, A-06/A-07 dependem dele); (4) M3 restante (semver/compat tests/redução de pub/remoção dos 19 shims); (5) células faltantes da matriz A-05 (opencode auth login + codex-via-acpx). Itens que precisam do Mario: login opencode; decisão sobre deny-escalation (finding #5.5); 3 itens `?` do inventário M0.

## Decisões operacionais do loop

- Reindex GitNexus adiado pro fim do M2 (extração invalida o índice a cada `git mv`); `detect_changes` roda mas é sinal fraco até lá.
- Padrão de falha recorrente: subagentes encerram deixando cargo em background (3 ocorrências) — mitigado com fingerprint + wakeup de estagnação + finalização inline.
- Máquina do owner caiu 2x com builds pesados — todos os builds em `CARGO_BUILD_JOBS=2`, um por vez.
- ENOSPC durante `cargo test` do 3b — liberados 23G (`target/debug/incremental` + deps antigos). Recorrente nesta máquina; se voltar, mesmo remédio.
- Session limit da API derrubou o 3b no meio do git mv (reset 19:20); retomado por SendMessage sem perda — renames staged sobreviveram.

## Pendências pro owner

- Inventário M0-03 tem 3 itens `?`: diretório `bastion/` tracked, `bastion.local.toml`/`.bastion/`, destino do `STRATEGY.md`.
- Login Codex/ChatGPT necessário pra validação live do `CodexAppServerRuntime` (A-03); acpx→Claude Code tentará validação com a auth local existente.
