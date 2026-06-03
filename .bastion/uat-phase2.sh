#!/usr/bin/env bash
# Phase-2 behavior UAT — drives the FULL pipeline via `bastion agent` (DEFAULT_OWNER),
# isolated DB+log in /tmp. Verifies routing/parallel/cabinet/egress from logs.
set -u
export PATH="$HOME/.cargo/bin:$PATH"
export RUST_LOG=info
export BASTION_DB=/tmp/bastion-uat.db
export BASTION_LOG=/tmp/bastion-uat.log
rm -f /tmp/bastion-uat.db* ; : > "$BASTION_LOG"

run_case() {
  local name="$1" msg="$2"
  local before; before=$(wc -l < "$BASTION_LOG")
  echo "════════ $name"
  echo "MSG: $msg"
  local out; out=$(cargo run -q -- agent --message "$msg" 2>/tmp/uat-err.txt)
  local rc=$?
  echo "REPLY: $(echo "$out" | head -c 300)"
  [ $rc -ne 0 ] && echo "ERR(exit $rc): $(grep -aoE 'Privacy egress blocked[^"]*|guardrail[^"]*|Error[^"]*' /tmp/uat-err.txt | head -1)"
  echo "LOG:"
  tail -n +$((before+1)) "$BASTION_LOG" \
    | grep -aoE '"event":"(router_decision|router_safe_fallback)"|"personas":\[[^]]*\]|"mode":"[A-Za-z]+"|"convene_reason":[^,}]*|Privacy egress blocked[^"]*' \
    | sed 's/^/  /'
  echo ""
}

run_case "1-SINGLE (carreira)"   "como organizo minhas reuniões de trabalho essa semana?"
run_case "2-SINGLE (negocio)"    "me dá 3 ideias de conteúdo pra KATANA"
run_case "3-PARALLEL (multi)"    "tô ansioso com o prazo do trabalho e ainda atrasando meus treinos"
run_case "4-CABINET (alto risco)" "tô pensando em largar meu emprego pra viver só da KATANA, vale a pena?"
run_case "5-EGRESS (saude local-only)" "monta uma dieta de 1700kcal só com o que eu gosto"
echo "DONE."
