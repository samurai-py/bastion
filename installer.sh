#!/usr/bin/env bash
# Bastion v2 — Installer Script
# Usage: bash <(curl -fsSL https://bastion.run/install)

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

_env_get() {
  local key="$1"
  grep -E "^${key}=" .env 2>/dev/null | cut -d'=' -f2- | tr -d '"' | tr -d "'" || true
}

_env_set() {
  local key="$1"
  local val="$2"
  if grep -qE "^${key}=" .env 2>/dev/null; then
    sed -i "s|^${key}=.*|${key}=${val}|" .env
  else
    echo "${key}=${val}" >> .env
  fi
}

_ask() {
  # Portable prompt — works on bash and sh
  local prompt="$1"
  local varname="$2"
  printf "%b" "$prompt"
  read -r "$varname"
}

# ── 1. Check prerequisites ────────────────────────────────────────
step "Checking prerequisites..."

install_docker() {
  warn "Docker not found."
  echo ""

  if [[ "$OSTYPE" == "darwin"* ]]; then
    if command -v brew &>/dev/null; then
      _ask "$(echo -e "${YELLOW}Instalar Docker Desktop via Homebrew? [s/N]: ${RESET}")" confirm
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
      exit 1
    fi

  elif grep -qi microsoft /proc/version 2>/dev/null; then
    _ask "$(echo -e "${YELLOW}Instalar Docker Engine no WSL2? [s/N]: ${RESET}")" confirm
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
    _ask "$(echo -e "${YELLOW}Instalar Docker automaticamente? [s/N]: ${RESET}")" confirm
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
    warn "Docker Compose não encontrado — instalando..."
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

# Stop any running containers before reconfiguring
if [ -f "docker-compose.yml" ]; then
  docker compose down --remove-orphans 2>/dev/null || true
fi

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

EXISTING_LLM=$(_env_get "OPENROUTER_API_KEY")$(_env_get "ANTHROPIC_API_KEY")$(_env_get "OPENAI_API_KEY")$(_env_get "GEMINI_API_KEY")$(_env_get "GROQ_API_KEY")

if [ -z "$EXISTING_LLM" ]; then
  echo ""
  echo "Qual LLM você quer usar?"
  echo "  1) OpenRouter    — acesso a dezenas de modelos, incluindo gratuitos (recomendado)"
  echo "  2) Groq          — gratuito, rápido, mas com limítes rígidos de contexto no plano free."
  echo "  3) Google Gemini — gratuito com limites generosos"
  echo "  4) Anthropic Claude — melhor qualidade, pago"
  echo "  5) OpenAI GPT    — popular, pago"
  echo ""
  _ask "$(echo -e "${CYAN}Escolha [1-5]: ${RESET}")" llm_choice

  case "$llm_choice" in
    1)
      info "Crie sua chave gratuita em: https://openrouter.ai/keys"
      _ask "$(echo -e "${CYAN}Cole sua OPENROUTER_API_KEY: ${RESET}")" llm_key
      _env_set "OPENROUTER_API_KEY" "$llm_key"
      success "OpenRouter configurado."
      ;;
    2)
      info "Crie sua chave gratuita em: https://console.groq.com"
      _ask "$(echo -e "${CYAN}Cole sua GROQ_API_KEY: ${RESET}")" llm_key
      _env_set "GROQ_API_KEY" "$llm_key"
      success "Groq configurado."
      ;;
    3)
      info "Crie sua chave em: https://aistudio.google.com/app/apikey"
      _ask "$(echo -e "${CYAN}Cole sua GEMINI_API_KEY: ${RESET}")" llm_key
      _env_set "GEMINI_API_KEY" "$llm_key"
      success "Gemini configurado."
      ;;
    4)
      info "Crie sua chave em: https://console.anthropic.com"
      _ask "$(echo -e "${CYAN}Cole sua ANTHROPIC_API_KEY: ${RESET}")" llm_key
      _env_set "ANTHROPIC_API_KEY" "$llm_key"
      success "Anthropic configurado."
      ;;
    5)
      info "Crie sua chave em: https://platform.openai.com/api-keys"
      _ask "$(echo -e "${CYAN}Cole sua OPENAI_API_KEY: ${RESET}")" llm_key
      _env_set "OPENAI_API_KEY" "$llm_key"
      success "OpenAI configurado."
      ;;
    *)
      warn "Opção inválida. Configure manualmente em .env depois."
      ;;
  esac
else
  success "LLM já configurado no .env."
fi

# ── 5. Interactive channel setup ─────────────────────────────────
step "Configurando canais de mensagens..."

echo ""
echo "Qual canal você quer configurar primeiro?"
echo "  1) Telegram"
echo "  2) WhatsApp (via Evolution API)"
echo "  3) Discord"
echo "  4) Slack"
echo "  5) Pular (configurar depois)"
echo ""
_ask "$(echo -e "${CYAN}Escolha [1-5]: ${RESET}")" channel_choice

case "$channel_choice" in
  1)
    info "Crie um bot no Telegram: abra @BotFather e use /newbot"
    _ask "$(echo -e "${CYAN}Cole seu TELEGRAM_BOT_TOKEN: ${RESET}")" tg_token
    if [ -n "$tg_token" ]; then
      _env_set "TELEGRAM_BOT_TOKEN" "$tg_token"
      info "Obtenha seu Telegram user ID: envie uma mensagem para @userinfobot"
      _ask "$(echo -e "${CYAN}Cole seu Telegram user ID: ${RESET}")" tg_user_id
      _env_set "TELEGRAM_USER_ID" "$tg_user_id"
      _env_set "PRIMARY_CHANNEL" "telegram"
      success "Telegram configurado."
    fi
    ;;
  2)
    info "Configure Evolution API: https://doc.evolution-api.com/v2/pt/get-started/introduction"
    _ask "$(echo -e "${CYAN}Cole a URL da sua Evolution API: ${RESET}")" wa_url
    _ask "$(echo -e "${CYAN}Cole sua Evolution API Key: ${RESET}")" wa_key
    _ask "$(echo -e "${CYAN}Cole seu número WhatsApp (com DDI, ex: 5521999999999): ${RESET}")" wa_number
    if [ -n "$wa_url" ] && [ -n "$wa_key" ] && [ -n "$wa_number" ]; then
      _env_set "WHATSAPP_API_URL" "$wa_url"
      _env_set "WHATSAPP_API_KEY" "$wa_key"
      _env_set "WHATSAPP_NUMBER" "$wa_number"
      _env_set "PRIMARY_CHANNEL" "whatsapp"
      success "WhatsApp configurado."
    fi
    ;;
  3)
    info "Crie um bot no Discord: https://discord.com/developers/applications"
    _ask "$(echo -e "${CYAN}Cole seu DISCORD_BOT_TOKEN: ${RESET}")" dc_token
    _ask "$(echo -e "${CYAN}Cole seu Discord user ID: ${RESET}")" dc_user_id
    if [ -n "$dc_token" ] && [ -n "$dc_user_id" ]; then
      _env_set "DISCORD_BOT_TOKEN" "$dc_token"
      _env_set "DISCORD_USER_ID" "$dc_user_id"
      _env_set "PRIMARY_CHANNEL" "discord"
      success "Discord configurado."
    fi
    ;;
  4)
    info "Configure Slack App: https://api.slack.com/apps"
    _ask "$(echo -e "${CYAN}Cole seu SLACK_BOT_TOKEN: ${RESET}")" slack_token
    _ask "$(echo -e "${CYAN}Cole seu Slack user ID: ${RESET}")" slack_user_id
    if [ -n "$slack_token" ] && [ -n "$slack_user_id" ]; then
      _env_set "SLACK_BOT_TOKEN" "$slack_token"
      _env_set "SLACK_USER_ID" "$slack_user_id"
      _env_set "PRIMARY_CHANNEL" "slack"
      success "Slack configurado."
    fi
    ;;
  5)
    warn "Nenhum canal configurado. Configure em .env depois."
    ;;
  *)
    warn "Opção inválida. Configure manualmente em .env depois."
    ;;
esac

# ── 6. Read .env values ───────────────────────────────────────────
ANTHROPIC_KEY=$(_env_get "ANTHROPIC_API_KEY")
OPENAI_KEY=$(_env_get "OPENAI_API_KEY")
GEMINI_KEY=$(_env_get "GEMINI_API_KEY")
GROQ_KEY=$(_env_get "GROQ_API_KEY")
OPENROUTER_KEY=$(_env_get "OPENROUTER_API_KEY")
PRIMARY_CHANNEL=$(_env_get "PRIMARY_CHANNEL")

# ── 7. Detect LLM provider ────────────────────────────────────────
step "Detecting LLM provider..."

if [ -n "$OPENROUTER_KEY" ]; then
  PROVIDER_ID="openrouter"
  PROVIDER_BASE_URL="https://openrouter.ai/api/v1"
  PROVIDER_API_KEY="$OPENROUTER_KEY"
  MODEL_ID="openai/gpt-oss-20b:free"
  MODEL_NAME="GPT-OSS 20B Free (OpenRouter)"
  PROVIDER_HEADERS='"headers": { "HTTP-Referer": "https://github.com/samurai-py/bastion", "X-Title": "Bastion" },'
  success "Using OpenRouter (GPT-OSS 20B Free)"
elif [ -n "$ANTHROPIC_KEY" ]; then
  PROVIDER_ID="anthropic"
  PROVIDER_BASE_URL="https://api.anthropic.com"
  PROVIDER_API_KEY="$ANTHROPIC_KEY"
  MODEL_ID="claude-sonnet-4-5"
  MODEL_NAME="Claude Sonnet"
  PROVIDER_HEADERS=""
  success "Using Anthropic (Claude)"
elif [ -n "$OPENAI_KEY" ]; then
  PROVIDER_ID="openai"
  PROVIDER_BASE_URL="https://api.openai.com/v1"
  PROVIDER_API_KEY="$OPENAI_KEY"
  MODEL_ID="gpt-4o"
  MODEL_NAME="GPT-4o"
  PROVIDER_HEADERS=""
  success "Using OpenAI (GPT-4o)"
elif [ -n "$GEMINI_KEY" ]; then
  PROVIDER_ID="google-gemini"
  PROVIDER_BASE_URL="https://generativelanguage.googleapis.com/v1beta/openai"
  PROVIDER_API_KEY="$GEMINI_KEY"
  MODEL_ID="gemini-2.0-flash"
  MODEL_NAME="Gemini 2.0 Flash"
  PROVIDER_HEADERS=""
  success "Using Google Gemini"
elif [ -n "$GROQ_KEY" ]; then
  PROVIDER_ID="groq"
  PROVIDER_BASE_URL="https://api.groq.com/openai/v1"
  PROVIDER_API_KEY="$GROQ_KEY"
  MODEL_ID="llama-3.3-70b-versatile"
  MODEL_NAME="Llama 3.3 70B (Groq)"
  PROVIDER_HEADERS=""
  success "Using Groq (Llama 3.3)"
else
  error "Nenhuma API key de LLM encontrada. Configure em .env e rode novamente."
  exit 1
fi

# ── 8. Generate openclaw.json ─────────────────────────────────────
step "Generating OpenClaw configuration..."

CONFIG_DIR="$INSTALL_DIR/config"
mkdir -p "$CONFIG_DIR"

cat > "$CONFIG_DIR/openclaw.json" <<EOF
{
  "agents": {
    "defaults": {
      "maxConcurrent": 4,
      "subagents": { "maxConcurrent": 8 },
      "compaction": { "mode": "safeguard" },
      "model": { "primary": "${PROVIDER_ID}/${MODEL_ID}" },
      "models": { "${PROVIDER_ID}/${MODEL_ID}": { "alias": "${PROVIDER_ID}" } }
    }
  },
  "gateway": {
    "mode": "local"
  },
  "models": {
    "mode": "merge",
    "providers": {
      "${PROVIDER_ID}": {
        "baseUrl": "${PROVIDER_BASE_URL}",
        "api": "openai-completions",
        "apiKey": "${PROVIDER_API_KEY}",
        ${PROVIDER_HEADERS}
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

# ── 9. Configure messaging channels ──────────────────────────────
mkdir -p "$CONFIG_DIR/channels"

case "$PRIMARY_CHANNEL" in
  telegram)
    TELEGRAM_TOKEN=$(_env_get "TELEGRAM_BOT_TOKEN")
    TELEGRAM_USER_ID=$(_env_get "TELEGRAM_USER_ID")
    if [ -n "$TELEGRAM_TOKEN" ]; then
      ALLOW_FROM=""
      [ -n "$TELEGRAM_USER_ID" ] && ALLOW_FROM="\"allowFrom\": [\"${TELEGRAM_USER_ID}\"],"
      cat > "$CONFIG_DIR/channels/telegram.json" <<EOF
{
  "enabled": true,
  "token": "${TELEGRAM_TOKEN}",
  ${ALLOW_FROM}
  "dmPolicy": "allowlist"
}
EOF
      success "Telegram channel configured."
    fi
    ;;
  whatsapp)
    WA_URL=$(_env_get "WHATSAPP_API_URL")
    WA_KEY=$(_env_get "WHATSAPP_API_KEY")
    WA_NUMBER=$(_env_get "WHATSAPP_NUMBER")
    if [ -n "$WA_URL" ] && [ -n "$WA_KEY" ]; then
      cat > "$CONFIG_DIR/channels/whatsapp.json" <<EOF
{
  "enabled": true,
  "apiUrl": "${WA_URL}",
  "apiKey": "${WA_KEY}",
  "allowFrom": ["${WA_NUMBER}"],
  "dmPolicy": "allowlist"
}
EOF
      success "WhatsApp channel configured."
    fi
    ;;
  discord)
    DISCORD_TOKEN=$(_env_get "DISCORD_BOT_TOKEN")
    DISCORD_USER_ID=$(_env_get "DISCORD_USER_ID")
    if [ -n "$DISCORD_TOKEN" ]; then
      ALLOW_FROM=""
      [ -n "$DISCORD_USER_ID" ] && ALLOW_FROM="\"allowFrom\": [\"${DISCORD_USER_ID}\"],"
      cat > "$CONFIG_DIR/channels/discord.json" <<EOF
{
  "enabled": true,
  "token": "${DISCORD_TOKEN}",
  ${ALLOW_FROM}
  "dmPolicy": "allowlist"
}
EOF
      success "Discord channel configured."
    fi
    ;;
  slack)
    SLACK_TOKEN=$(_env_get "SLACK_BOT_TOKEN")
    SLACK_USER_ID=$(_env_get "SLACK_USER_ID")
    if [ -n "$SLACK_TOKEN" ]; then
      ALLOW_FROM=""
      [ -n "$SLACK_USER_ID" ] && ALLOW_FROM="\"allowFrom\": [\"${SLACK_USER_ID}\"],"
      cat > "$CONFIG_DIR/channels/slack.json" <<EOF
{
  "enabled": true,
  "token": "${SLACK_TOKEN}",
  ${ALLOW_FROM}
  "dmPolicy": "allowlist"
}
EOF
      success "Slack channel configured."
    fi
    ;;
esac

# ── 10. Create required directories and fix permissions ───────────
mkdir -p "$INSTALL_DIR/personas" "$INSTALL_DIR/tmp"
chmod 1777 "$INSTALL_DIR/tmp"
docker run --rm -v "$INSTALL_DIR/config:/data" alpine chown -R 1000:1000 /data 2>/dev/null || true

# ── 10b. Pre-authorize primary channel user in USER.md ───────────
PRIMARY_CHANNEL=$(_env_get "PRIMARY_CHANNEL")
USER_ID=""

case "$PRIMARY_CHANNEL" in
  telegram) USER_ID=$(_env_get "TELEGRAM_USER_ID") ;;
  whatsapp) USER_ID=$(_env_get "WHATSAPP_NUMBER") ;;
  discord) USER_ID=$(_env_get "DISCORD_USER_ID") ;;
  slack) USER_ID=$(_env_get "SLACK_USER_ID") ;;
esac

if [ -n "$USER_ID" ]; then
  cat > "$INSTALL_DIR/USER.md" <<EOF
---
name: ""
language: "pt-BR"
authorized_user_ids:
  - "${USER_ID}"
personas: []
totp_configured: false
---

<!-- Este arquivo é gerado automaticamente pelo skill bastion/onboarding. -->
EOF
  success "User ID pré-autorizado no USER.md."
fi

# ── 10c. Sync Bastion files into OpenClaw workspace ───────────────
WORKSPACE_DIR="$INSTALL_DIR/config/workspace"
mkdir -p "$WORKSPACE_DIR/skills"

for f in SOUL.md USER.md AGENTS.md HEARTBEAT.md; do
  [ -f "$INSTALL_DIR/$f" ] && cp "$INSTALL_DIR/$f" "$WORKSPACE_DIR/$f"
done

for skill_dir in "$INSTALL_DIR/skills/"/*/; do
  skill_name=$(basename "$skill_dir")
  [ -f "$skill_dir/SKILL.md" ] && cp "$skill_dir/SKILL.md" "$WORKSPACE_DIR/skills/${skill_name}.md"
done

success "Bastion context synced to OpenClaw workspace."

# ── 11. Start / restart Bastion ──────────────────────────────────
step "Starting Bastion..."

cd "$INSTALL_DIR"
docker compose pull --quiet
docker compose up -d --force-recreate
success "Bastion is running."

# ── 12. Done ──────────────────────────────────────────────────────
step "Installation complete"

echo ""
echo -e "${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
echo -e "  Model:    ${BOLD}${MODEL_NAME}${RESET}"
[ -n "$PRIMARY_CHANNEL" ] && echo -e "  Channel:  ${BOLD}${PRIMARY_CHANNEL} ✓${RESET}"
echo ""
case "$PRIMARY_CHANNEL" in
  telegram) echo -e "  Open your Telegram bot and send ${BOLD}/start${RESET}" ;;
  whatsapp) echo -e "  Send a message to your WhatsApp number to start" ;;
  discord) echo -e "  Send a DM to your Discord bot to start" ;;
  slack) echo -e "  Send a DM to your Slack bot to start" ;;
esac
echo -e "${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
echo ""
