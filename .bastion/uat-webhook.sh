#!/usr/bin/env bash
# Phase-2 webhook UAT: start daemon with webhook+owner-map (isolated DB), POST valid +
# denied requests, check status/body, then shut down.
set -u
export PATH="$HOME/.cargo/bin:$PATH"
export RUST_LOG=info
export BASTION_DB=/tmp/bastion-wh.db
export BASTION_LOG=/tmp/bastion-wh.log
export BASTION_WEBHOOK_ADDR=127.0.0.1:8787
export BASTION_WEBHOOK_OWNERS="s3cret:mario"
# NO telegram for this run (avoid 409 with the other instance)
unset TELEGRAM_BOT_TOKEN
rm -f /tmp/bastion-wh.db* ; : > "$BASTION_LOG"

# Keep stdin open so the daemon REPL loop stays alive while serving the webhook.
tail -f /dev/null | cargo run -q -- daemon >/tmp/wh-stdout.txt 2>/tmp/wh-stderr.txt &
DPID=$!
cleanup() { kill "$DPID" 2>/dev/null; pkill -f "target/debug/bastion daemon" 2>/dev/null; }
trap cleanup EXIT

# Wait for webhook to bind
for i in $(seq 1 30); do
  grep -aq 'webhook_started' "$BASTION_LOG" && break
  sleep 1
done
grep -aq 'webhook_started' "$BASTION_LOG" && echo "webhook UP" || { echo "webhook NÃO subiu"; tail -5 "$BASTION_LOG"; exit 1; }

echo ""
echo "════════ A) POST válido (x-bastion-token: s3cret → owner mario)"
curl -s -o /tmp/wh-a.body -w "HTTP %{http_code}\n" -X POST "http://$BASTION_WEBHOOK_ADDR/webhook" \
  -H 'x-bastion-token: s3cret' -H 'content-type: application/json' \
  -d '{"text":"me dá 3 ideias de conteúdo pra KATANA"}'
echo "BODY: $(head -c 300 /tmp/wh-a.body)"

echo ""
echo "════════ B) POST sem token (sender não mapeado → deve NEGAR, não 200)"
curl -s -o /tmp/wh-b.body -w "HTTP %{http_code}\n" -X POST "http://$BASTION_WEBHOOK_ADDR/webhook" \
  -H 'content-type: application/json' -d '{"text":"oi"}'
echo "BODY: $(head -c 200 /tmp/wh-b.body)"

echo ""
echo "════════ C) POST token inválido (não mapeado → deve NEGAR)"
curl -s -o /tmp/wh-c.body -w "HTTP %{http_code}\n" -X POST "http://$BASTION_WEBHOOK_ADDR/webhook" \
  -H 'x-bastion-token: errado' -H 'content-type: application/json' -d '{"text":"oi"}'
echo "BODY: $(head -c 200 /tmp/wh-c.body)"

echo ""
echo "════════ LOG (webhook + router)"
grep -aoE '"event":"(webhook_started|router_decision)"|"mode":"[A-Za-z]+"|"personas":\[[^]]*\]' "$BASTION_LOG" | sed 's/^/  /'
echo "DONE."
