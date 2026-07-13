# Loop autônomo — relatório de execução e findings

> Corrida iniciada 2026-07-13. Itens que exigem olho do owner ou follow-up marcados por seção.

## Findings de review (não bloqueantes, endereçados no fluxo)

1. **[M3] `ToolSource::call_tool_with_timeout` é ungated por construção.** Os dois call sites pré-existentes de bypass do registry (fallback de registry vazio em `dispatch_tool_loop` e o tool-loop inteiro de `run_provider_fallback`) aplicam `check_egress` manualmente ANTES de chamar (WR-02/D-13, verificado). O trait documenta isso, mas nada impede um caller futuro de esquecer o gate. Follow-up M3-01/M3-02: mover o gate pra dentro da implementação (ou expor só um wrapper gated) e cobrir com teste de invariante. Além do egress: esses paths não passam pelo trust-tagging do `TaggedValue` que `registry.invoke` aplica — paridade de trust é gap pré-existente, avaliar no M3-02.
2. **[3b] `GoalPort` retornava `crate::goal::Goal` na assinatura do kernel** — vazamento de tipo de cognição. Resolvido no 3b movendo `Goal` (e `PrivacyTier`) pra `bastion-types`.

## Decisões operacionais do loop

- Reindex GitNexus adiado pro fim do M2 (extração invalida o índice a cada `git mv`); `detect_changes` roda mas é sinal fraco até lá.
- Padrão de falha recorrente: subagentes encerram deixando cargo em background (3 ocorrências) — mitigado com fingerprint + wakeup de estagnação + finalização inline.
- Máquina do owner caiu 2x com builds pesados — todos os builds em `CARGO_BUILD_JOBS=2`, um por vez.

## Pendências pro owner

- Inventário M0-03 tem 3 itens `?`: diretório `bastion/` tracked, `bastion.local.toml`/`.bastion/`, destino do `STRATEGY.md`.
- Login Codex/ChatGPT necessário pra validação live do `CodexAppServerRuntime` (A-03); acpx→Claude Code tentará validação com a auth local existente.
