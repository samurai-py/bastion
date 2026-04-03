#!/usr/bin/env bash
# Bastion v2 — Installer Script
# Usage: curl -fsSL https://bastion.run/install | bash

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

install_docker() {
  warn "Docker not found."
  echo ""

  if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS
    if command -v brew &>/dev/null; then
      read -rp "$(echo -e "${YELLOW}Instalar Docker Desktop via Homebrew? [s/N]:${RESET} ")" confirm
      if [[ "$confirm" =~ ^[sS]$ ]]; then
        brew install --cask docker
        info "Abrindo Docker Desktop..."
        open -a Docker
        info "Aguarde o Docker iniciar e rode o installer novamente."
        exit 0
      else
        error "Docker é obrigatório. Instale em: https://docs.docker.com/desktop/mac/install/"
        exit 1
      fi
    else
      error "Instale o Docker Desktop para Mac: https://docs.docker.com/desktop/mac/install/"
      open "https://docs.docker.com/desktop/mac/install/" 2>/dev/null || true
      exit 1
    fi

  elif grep -qi microsoft /proc/version 2>/dev/null; then
    # WSL2
    read -rp "$(echo -e "${YELLOW}Instalar Docker Engine no WSL2? [s/N]:${RESET} ")" confirm
    if [[ "$confirm" =~ ^[sS]$ ]]; then
      curl -fsSL https://get.docker.com | sh
      sudo usermod -aG docker "$USER"
      success "Docker instalado. Reinicie o terminal e rode o installer novamente."
      exit 0
    else
      error "Docker é obrigatório para rodar o Bastion."
      exit 1
    fi

  else
    # Linux genérico
    read -rp "$(echo -e "${YELLOW}Instalar Docker automaticamente? [s/N]:${RESET} ")" confirm
    if [[ "$confirm" =~ ^[sS]$ ]]; then
      curl -fsSL https://get.docker.com | sh
      sudo usermod -aG docker "$USER"
      success "Docker instalado. Reinicie o terminal e rode o installer novamente."
      exit 0
    else
      error "Docker é obrigatório. Instale em: https://docs.docker.com/get-docker/"
      exit 1
    fi
  fi
}

check_docker() {
  if command -v docker &>/dev/null; then
    success "Docker found: $(command -v docker)"
  else
    install_docker
  fi
}

check_docker_compose() {
  if docker compose version &>/dev/null 2>&1; then
    success "Docker Compose found (plugin)"
  elif command -v docker-compose &>/dev/null; then
    success "Docker Compose found: $(command -v docker-compose)"
  else
    warn "Docker Compose plugin not found — installing..."
    # Compose v2 is bundled with Docker Engine >= 20.10 via get.docker.com
    # If still missing, install manually
    COMPOSE_VERSION="v2.27.0"
    COMPOSE_DIR="${HOME}/.docker/cli-plugins"
    mkdir -p "$COMPOSE_DIR"
    curl -fsSL "https://github.com/docker/compose/releases/download/${COMPOSE_VERSION}/docker-compose-$(uname -s)-$(uname -m)" \
      -o "$COMPOSE_DIR/docker-compose"
    chmod +x "$COMPOSE_DIR/docker-compose"
    success "Docker Compose installed."
  fi
}

check_docker
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

# ── 4. Interactive LLM setup ─────────────────────────────────────
step "Configurando LLM..."

# Check if any LLM key is already set
EXISTING_LLM=$(_env_get "ANTHROPIC_API_KEY")$(_env_get "OPENAI_API_KEY")$(_env_get "GEMINI_API_KEY")$(_env_get "GROQ_API_KEY")

if [ -z "$EXISTING_LLM" ]; then
  echo ""
  echo "Qual LLM você quer usar?"
  echo "  1) Groq       — gratuito, rápido (recomendado para começar)"
  echo "  2) Google Gemini — gratuito com limites generosos"
  echo "  3) Anthropic Claude — melhor qualidade, pago"
  echo "  4) OpenAI GPT — popular, pago"
  echo ""
  read -rp "$(echo -e "${CYAN}Escolha [1-4]:${RESET} ")" llm_choice

  case "$llm_choice" in
    1)
      echo ""
      info "Crie sua chave gratuita em: https://console.groq.com"
      read -rp "$(echo -e "${CYAN}Cole sua GROQ_API_KEY:${RESET} ")" llm_key
      sed -i "s|^GROQ_API_KEY=.*|GROQ_API_KEY=${llm_key}|" .env
      success "Groq configurado."
      ;;
    2)
      echo ""
      info "Crie sua chave em: https://aistudio.google.com/app/apikey"
      read -rp "$(echo -e "${CYAN}Cole sua GEMINI_API_KEY:${RESET} ")" llm_key
      sed -i "s|^GEMINI_API_KEY=.*|GEMINI_API_KEY=${llm_key}|" .env
      success "Gemini configurado."
      ;;
    3)
      echo ""
      info "Crie sua chave em: https://console.anthropic.com"
      read -rp "$(echo -e "${CYAN}Cole sua ANTHROPIC_API_KEY:${RESET} ")" llm_key
      sed -i "s|^ANTHROPIC_API_KEY=.*|ANTHROPIC_API_KEY=${llm_key}|" .env
      success "Anthropic configurado."
      ;;
    4)
      echo ""
      info "Crie sua chave em: https://platform.openai.com/api-keys"
      read -rp "$(echo -e "${CYAN}Cole sua OPENAI_API_KEY:${RESET} ")" llm_key
      sed -i "s|^OPENAI_API_KEY=.*|OPENAI_API_KEY=${llm_key}|" .env
      success "OpenAI configurado."
      ;;
    *)
      warn "Opção inválida. Configure manualmente em .env depois."
      ;;
  esac
else
  success "LLM já configurado no .env."
fi

# ── 5. Interactive Telegram setup ────────────────────────────────
step "Configurando canal de mensagens..."

EXISTING_TG=$(_env_get "TELEGRAM_BOT_TOKEN")

if [ -z "$EXISTING_TG" ]; then
  echo ""
  echo "Você tem um bot no Telegram?"
  echo "  Se não tiver: abra o Telegram, fale com @BotFather e crie um bot."
  echo ""
  read -rp "$(echo -e "${CYAN}Cole seu TELEGRAM_BOT_TOKEN (ou Enter para pular):${RESET} ")" tg_token
  if [ -n "$tg_token" ]; then
    sed -i "s|^TELEGRAM_BOT_TOKEN=.*|TELEGRAM_BOT_TOKEN=${tg_token}|" .env
    success "Telegram configurado."
  else
    warn "Telegram não configurado. Configure em .env depois."
  fi
else
  success "Telegram já configurado no .env."
fi
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
