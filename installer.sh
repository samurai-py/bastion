#!/usr/bin/env bash
# Bastion v2 — Installer Script
# Usage: curl -fsSL https://get.bastion.ai | bash

set -euo pipefail

REPO_URL="https://github.com/samurai-py/bastion.git"
INSTALL_DIR="${BASTION_DIR:-$HOME/bastion}"

# ── Colors ────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
RESET='\033[0m'

info()    { echo -e "${CYAN}[bastion]${RESET} $*"; }
success() { echo -e "${GREEN}[bastion]${RESET} $*"; }
warn()    { echo -e "${YELLOW}[bastion]${RESET} $*"; }
error()   { echo -e "${RED}[bastion] ERROR:${RESET} $*" >&2; }
step()    { echo -e "\n${BOLD}▶ $*${RESET}"; }

# ── 1. Check prerequisites ────────────────────────────────────────
step "Checking prerequisites..."

check_command() {
  local cmd="$1"
  local install_hint="$2"
  if command -v "$cmd" &>/dev/null; then
    success "$cmd found: $(command -v "$cmd")"
  else
    error "$cmd is not installed. $install_hint"
    exit 1
  fi
}

check_docker_compose() {
  if docker compose version &>/dev/null 2>&1; then
    success "Docker Compose found: docker compose (plugin)"
  elif command -v docker-compose &>/dev/null; then
    success "Docker Compose found: $(command -v docker-compose)"
  else
    error "Docker Compose is not installed."
    error "Install it from: https://docs.docker.com/compose/install/"
    exit 1
  fi
}

check_command "docker" "Install Docker from: https://docs.docker.com/get-docker/"
check_docker_compose

if ! docker info &>/dev/null 2>&1; then
  error "Docker daemon is not running. Please start Docker and try again."
  exit 1
fi
success "Docker daemon is running."

# ── 2. Clone or update repository ────────────────────────────────
step "Setting up Bastion directory..."

if [ -d "$INSTALL_DIR/.git" ]; then
  warn "Repository already exists at $INSTALL_DIR — skipping clone."
else
  info "Cloning Bastion into $INSTALL_DIR ..."
  git clone "$REPO_URL" "$INSTALL_DIR"
  success "Repository cloned successfully."
fi

cd "$INSTALL_DIR"

# ── 3. Copy .env.example → .env (idempotent) ─────────────────────
step "Configuring environment..."

if [ -f ".env" ]; then
  warn ".env already exists — preserving your configuration."
else
  cp .env.example .env
  success ".env created from .env.example."
fi

# ── 4. Read .env values ───────────────────────────────────────────
_env_get() {
  local key="$1"
  grep -E "^${key}=" .env 2>/dev/null | cut -d'=' -f2- | tr -d '"' | tr -d "'" || true
}

ANTHROPIC_KEY=$(_env_get "ANTHROPIC_API_KEY")
OPENAI_KEY=$(_env_get "OPENAI_API_KEY")
GEMINI_KEY=$(_env_get "GEMINI_API_KEY")
GROQ_KEY=$(_env_get "GROQ_API_KEY")
TELEGRAM_TOKEN=$(_env_get "TELEGRAM_BOT_TOKEN")

# ── 5. Detect LLM provider ────────────────────────────────────────
step "Detecting LLM provider..."

PROVIDER_ID=""
PROVIDER_BASE_URL=""
PROVIDER_API_KEY=""
MODEL_ID=""
MODEL_NAME=""

if [ -n "$ANTHROPIC_KEY" ]; then
  PROVIDER_ID="anthropic"
  PROVIDER_BASE_URL="https://api.anthropic.com"
  PROVIDER_API_KEY="$ANTHROPIC_KEY"
  MODEL_ID="claude-sonnet-4-5"
  MODEL_NAME="Claude Sonnet"
  success "Using Anthropic (Claude)"
elif [ -n "$OPENAI_KEY" ]; then
  PROVIDER_ID="openai"
  PROVIDER_BASE_URL="https://api.openai.com/v1"
  PROVIDER_API_KEY="$OPENAI_KEY"
  MODEL_ID="gpt-4o"
  MODEL_NAME="GPT-4o"
  success "Using OpenAI (GPT-4o)"
elif [ -n "$GEMINI_KEY" ]; then
  PROVIDER_ID="google-gemini"
  PROVIDER_BASE_URL="https://generativelanguage.googleapis.com/v1beta/openai"
  PROVIDER_API_KEY="$GEMINI_KEY"
  MODEL_ID="gemini-2.0-flash"
  MODEL_NAME="Gemini 2.0 Flash"
  success "Using Google Gemini"
elif [ -n "$GROQ_KEY" ]; then
  PROVIDER_ID="groq"
  PROVIDER_BASE_URL="https://api.groq.com/openai/v1"
  PROVIDER_API_KEY="$GROQ_KEY"
  MODEL_ID="llama-3.3-70b-versatile"
  MODEL_NAME="Llama 3.3 70B (Groq)"
  success "Using Groq (Llama 3.3)"
else
  error "No LLM API key found in .env."
  error "Add at least one of: ANTHROPIC_API_KEY, OPENAI_API_KEY, GEMINI_API_KEY, GROQ_API_KEY"
  exit 1
fi

# ── 6. Generate openclaw.json ─────────────────────────────────────
step "Generating OpenClaw configuration..."

GATEWAY_TOKEN=$(openssl rand -hex 20 2>/dev/null || cat /proc/sys/kernel/random/uuid | tr -d '-')
CONFIG_DIR="$INSTALL_DIR/config"
mkdir -p "$CONFIG_DIR"

cat > "$CONFIG_DIR/openclaw.json" <<EOF
{
  "agents": {
    "defaults": {
      "maxConcurrent": 4,
      "subagents": { "maxConcurrent": 8 },
      "compaction": { "mode": "safeguard" },
      "model": {
        "primary": "${PROVIDER_ID}/${MODEL_ID}"
      },
      "models": {
        "${PROVIDER_ID}/${MODEL_ID}": { "alias": "${PROVIDER_ID}" }
      }
    }
  },
  "gateway": {
    "auth": { "mode": "token", "token": "${GATEWAY_TOKEN}" },
    "mode": "local"
  },
  "models": {
    "mode": "merge",
    "providers": {
      "${PROVIDER_ID}": {
        "baseUrl": "${PROVIDER_BASE_URL}",
        "api": "openai-completions",
        "apiKey": "${PROVIDER_API_KEY}",
        "models": [
          {
            "id": "${MODEL_ID}",
            "name": "${MODEL_NAME}",
            "contextWindow": 128000,
            "maxTokens": 8192,
            "input": ["text"],
            "cost": { "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0 },
            "reasoning": false
          }
        ]
      }
    }
  }
}
EOF

success "OpenClaw configuration generated."

# ── 7. Configure Telegram channel (if token present) ─────────────
if [ -n "$TELEGRAM_TOKEN" ]; then
  step "Configuring Telegram channel..."
  # Telegram config is stored separately — will be picked up on first start
  mkdir -p "$CONFIG_DIR/channels"
  cat > "$CONFIG_DIR/channels/telegram.json" <<EOF
{
  "enabled": true,
  "token": "${TELEGRAM_TOKEN}"
}
EOF
  success "Telegram channel configured."
fi

# ── 8. Create required directories ───────────────────────────────
mkdir -p "$INSTALL_DIR/personas" "$INSTALL_DIR/tmp"

# Fix tmp permissions for container user (uid 1000)
docker run --rm -v "$INSTALL_DIR/tmp:/tmp" alpine chown -R 1000:1000 /tmp 2>/dev/null || true
docker run --rm -v "$INSTALL_DIR/config:/data" alpine chown -R 1000:1000 /data 2>/dev/null || true

# ── 9. Done ───────────────────────────────────────────────────────
step "Installation complete"

echo ""
echo -e "${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
echo -e "  Model:    ${BOLD}${MODEL_NAME}${RESET}"
[ -n "$TELEGRAM_TOKEN" ] && echo -e "  Channel:  ${BOLD}Telegram ✓${RESET}"
echo ""
echo -e "  Start Bastion:"
echo -e "  ${CYAN}cd ${INSTALL_DIR} && docker compose up -d${RESET}"
echo ""
[ -n "$TELEGRAM_TOKEN" ] && echo -e "  Then open your Telegram bot and send ${BOLD}/start${RESET}"
echo -e "${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
echo ""
