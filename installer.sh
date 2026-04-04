#!/usr/bin/env bash
# ============================================================================
# BASTION INSTALLER — Agnóstico, Robusto e Orientado a Wizard
# ============================================================================
# Suporta configuração via:
#   1. Wizard interativo (padrão)
#   2. Variáveis de ambiente (CI/CD ou automação)
#   3. Arquivo .env existente (preserva configurações)
#
# Uso:
#   bash <(curl -fsSL https://bastion.run/install)
#   BASTION_WIZARD=false LLM_PROVIDER=anthropic bash installer.sh
# ============================================================================

set -euo pipefail

REPO_URL="https://github.com/samurai-py/bastion.git"
INSTALL_DIR="${BASTION_DIR:-$HOME/bastion}"
WIZARD_MODE="${BASTION_WIZARD:-true}"

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

banner() {
    echo -e "${BOLD}"
    echo "    ██████╗  █████╗ ███████╗████████╗██╗ ██████╗ ███╗   ██╗"
    echo "    ██╔══██╗██╔══██╗██╔════╝╚══██╔══╝██║██╔═══██╗████╗  ██║"
    echo "    ██████╔╝███████║███████╗   ██║   ██║██║   ██║██╔██╗ ██║"
    echo "    ██╔══██╗██╔══██║╚════██║   ██║   ██║██║   ██║██║╚██╗██║"
    echo "    ██████╔╝██║  ██║███████║   ██║   ██║╚██████╔╝██║ ╚████║"
    echo "    ╚═════╝ ╚═╝  ╚═╝╚══════╝   ╚═╝   ╚═╝ ╚═════╝ ╚═╝  ╚═══╝"
    echo -e "${RESET}"
    echo "    >> Instalador Agnóstico v3.0 | Self-Hosted Life OS"
    echo ""
}

# ── Utility Functions ─────────────────────────────────────────────
_env_get() {
  local key="$1"
  grep -E "^${key}=" .env 2>/dev/null | cut -d'=' -f2- | tr -d '"' | tr -d "'" || true
}

_env_set() {
  local key="$1"
  local val="$2"
  if grep -qE "^${key}=" .env 2>/dev/null; then
    sed -i.bak "s|^${key}=.*|${key}=${val}|" .env && rm -f .env.bak
  else
    echo "${key}=${val}" >> .env
  fi
}

_ask() {
  local prompt="$1"
  local varname="$2"
  printf "%b" "$prompt"
  read -r "$varname"
}

_ask_or_env() {
  # Usa variável de ambiente se existir, senão pergunta
  local prompt="$1"
  local varname="$2"
  local env_var="$3"
  
  if [ -n "${!env_var:-}" ]; then
    eval "$varname=\"${!env_var}\""
    info "Usando $env_var da variável de ambiente"
  elif [ "$WIZARD_MODE" = "true" ]; then
    _ask "$prompt" "$varname"
  else
    error "$env_var não definida e wizard desabilitado"
    exit 1
  fi
}

_select_or_env() {
  # Menu de seleção ou variável de ambiente
  local prompt="$1"
  local varname="$2"
  local env_var="$3"
  shift 3
  local options=("$@")
  
  if [ -n "${!env_var:-}" ]; then
    eval "$varname=\"${!env_var}\""
    info "Usando $env_var=${!env_var}"
    return
  fi
  
  if [ "$WIZARD_MODE" = "false" ]; then
    error "$env_var não definida e wizard desabilitado"
    exit 1
  fi
  
  echo ""
  echo "$prompt"
  PS3="Escolha [1-${#options[@]}]: "
  select opt in "${options[@]}"; do
    if [ -n "$opt" ]; then
      eval "$varname=\"$opt\""
      break
    fi
  done
}

# ── 1. Check prerequisites ────────────────────────────────────────
banner
step "Verificando pré-requisitos..."

install_docker() {
  warn "Docker não encontrado."
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
    success "Docker encontrado: $(docker --version | cut -d' ' -f3 | tr -d ',')"
  else
    install_docker
  fi
}

check_docker_compose() {
  if docker compose version &>/dev/null 2>&1; then
    success "Docker Compose encontrado (plugin)"
  elif command -v docker-compose &>/dev/null; then
    success "Docker Compose encontrado: $(command -v docker-compose)"
  else
    warn "Docker Compose não encontrado — instalando..."
    COMPOSE_VERSION="v2.27.0"
    COMPOSE_DIR="${HOME}/.docker/cli-plugins"
    mkdir -p "$COMPOSE_DIR"
    curl -fsSL "https://github.com/docker/compose/releases/download/${COMPOSE_VERSION}/docker-compose-$(uname -s)-$(uname -m)" \
      -o "$COMPOSE_DIR/docker-compose"
    chmod +x "$COMPOSE_DIR/docker-compose"
    success "Docker Compose instalado."
  fi
}

check_docker
check_docker_compose

if ! docker info &>/dev/null 2>&1; then
  error "Docker daemon não está rodando. Inicie o Docker e tente novamente."
  exit 1
fi
success "Docker daemon está ativo."

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

# ── 4. Configuração Dinâmica: LLM Provider ───────────────────────
step "Configurando LLM Provider..."

EXISTING_LLM=$(_env_get "OPENROUTER_API_KEY")$(_env_get "ANTHROPIC_API_KEY")$(_env_get "OPENAI_API_KEY")$(_env_get "GEMINI_API_KEY")$(_env_get "GROQ_API_KEY")

if [ -z "$EXISTING_LLM" ]; then
  _select_or_env "Qual LLM você quer usar?" llm_choice LLM_PROVIDER \
    "OpenRouter (recomendado — modelos gratuitos)" \
    "Groq (gratuito, rápido)" \
    "Google Gemini (gratuito)" \
    "Anthropic Claude (pago, melhor qualidade)" \
    "OpenAI GPT (pago, popular)"

  case "$llm_choice" in
    *OpenRouter*)
      info "Crie sua chave gratuita em: https://openrouter.ai/keys"
      _ask_or_env "$(echo -e "${CYAN}Cole sua OPENROUTER_API_KEY: ${RESET}")" llm_key OPENROUTER_API_KEY
      _env_set "OPENROUTER_API_KEY" "$llm_key"
      
      # Permite escolher o modelo do OpenRouter
      if [ "$WIZARD_MODE" = "true" ]; then
        _select_or_env "Qual modelo do OpenRouter?" model_choice OPENROUTER_MODEL \
          "openai/gpt-oss-20b:free (gratuito)" \
          "meta-llama/llama-3.3-70b-instruct:free (gratuito)" \
          "anthropic/claude-3.5-sonnet (pago)" \
          "openai/gpt-4o (pago)"
        
        case "$model_choice" in
          *gpt-oss*) _env_set "OPENROUTER_MODEL" "openai/gpt-oss-20b:free" ;;
          *llama*) _env_set "OPENROUTER_MODEL" "meta-llama/llama-3.3-70b-instruct:free" ;;
          *claude*) _env_set "OPENROUTER_MODEL" "anthropic/claude-3.5-sonnet" ;;
          *gpt-4o*) _env_set "OPENROUTER_MODEL" "openai/gpt-4o" ;;
        esac
      fi
      success "OpenRouter configurado."
      ;;
    *Groq*)
      info "Crie sua chave gratuita em: https://console.groq.com"
      _ask_or_env "$(echo -e "${CYAN}Cole sua GROQ_API_KEY: ${RESET}")" llm_key GROQ_API_KEY
      _env_set "GROQ_API_KEY" "$llm_key"
      success "Groq configurado."
      ;;
    *Gemini*)
      info "Crie sua chave em: https://aistudio.google.com/app/apikey"
      _ask_or_env "$(echo -e "${CYAN}Cole sua GEMINI_API_KEY: ${RESET}")" llm_key GEMINI_API_KEY
      _env_set "GEMINI_API_KEY" "$llm_key"
      success "Gemini configurado."
      ;;
    *Claude*)
      info "Crie sua chave em: https://console.anthropic.com"
      _ask_or_env "$(echo -e "${CYAN}Cole sua ANTHROPIC_API_KEY: ${RESET}")" llm_key ANTHROPIC_API_KEY
      _env_set "ANTHROPIC_API_KEY" "$llm_key"
      success "Anthropic configurado."
      ;;
    *OpenAI*)
      info "Crie sua chave em: https://platform.openai.com/api-keys"
      _ask_or_env "$(echo -e "${CYAN}Cole sua OPENAI_API_KEY: ${RESET}")" llm_key OPENAI_API_KEY
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

# ── 5. Configuração Dinâmica: Canal de Mensagens ─────────────────
step "Configurando canal de mensagens..."

PRIMARY_CHANNEL=$(_env_get "PRIMARY_CHANNEL")

if [ -z "$PRIMARY_CHANNEL" ]; then
  _select_or_env "Qual canal você quer configurar?" channel_choice PRIMARY_CHANNEL \
    "Telegram" \
    "WhatsApp (via Evolution API)" \
    "Discord" \
    "Slack" \
    "Pular (configurar depois)"

  case "$channel_choice" in
    Telegram)
      info "Crie um bot no Telegram: abra @BotFather e use /newbot"
      _ask_or_env "$(echo -e "${CYAN}Cole seu TELEGRAM_BOT_TOKEN: ${RESET}")" tg_token TELEGRAM_BOT_TOKEN
      if [ -n "$tg_token" ]; then
        _env_set "TELEGRAM_BOT_TOKEN" "$tg_token"
        info "Obtenha seu Telegram user ID: envie uma mensagem para @userinfobot"
        _ask_or_env "$(echo -e "${CYAN}Cole seu Telegram user ID: ${RESET}")" tg_user_id TELEGRAM_USER_ID
        _env_set "TELEGRAM_USER_ID" "$tg_user_id"
        _env_set "PRIMARY_CHANNEL" "telegram"
        success "Telegram configurado."
      fi
      ;;
    "WhatsApp (via Evolution API)")
      info "Configure Evolution API: https://doc.evolution-api.com/v2/pt/get-started/introduction"
      _ask_or_env "$(echo -e "${CYAN}Cole a URL da sua Evolution API: ${RESET}")" wa_url WHATSAPP_API_URL
      _ask_or_env "$(echo -e "${CYAN}Cole sua Evolution API Key: ${RESET}")" wa_key WHATSAPP_API_KEY
      _ask_or_env "$(echo -e "${CYAN}Cole seu número WhatsApp (com DDI, ex: 5521999999999): ${RESET}")" wa_number WHATSAPP_NUMBER
      if [ -n "$wa_url" ] && [ -n "$wa_key" ] && [ -n "$wa_number" ]; then
        _env_set "WHATSAPP_API_URL" "$wa_url"
        _env_set "WHATSAPP_API_KEY" "$wa_key"
        _env_set "WHATSAPP_NUMBER" "$wa_number"
        _env_set "PRIMARY_CHANNEL" "whatsapp"
        success "WhatsApp configurado."
      fi
      ;;
    Discord)
      info "Crie um bot no Discord: https://discord.com/developers/applications"
      _ask_or_env "$(echo -e "${CYAN}Cole seu DISCORD_BOT_TOKEN: ${RESET}")" dc_token DISCORD_BOT_TOKEN
      _ask_or_env "$(echo -e "${CYAN}Cole seu Discord user ID: ${RESET}")" dc_user_id DISCORD_USER_ID
      if [ -n "$dc_token" ] && [ -n "$dc_user_id" ]; then
        _env_set "DISCORD_BOT_TOKEN" "$dc_token"
        _env_set "DISCORD_USER_ID" "$dc_user_id"
        _env_set "PRIMARY_CHANNEL" "discord"
        success "Discord configurado."
      fi
      ;;
    Slack)
      info "Configure Slack App: https://api.slack.com/apps"
      _ask_or_env "$(echo -e "${CYAN}Cole seu SLACK_BOT_TOKEN: ${RESET}")" slack_token SLACK_BOT_TOKEN
      _ask_or_env "$(echo -e "${CYAN}Cole seu Slack user ID: ${RESET}")" slack_user_id SLACK_USER_ID
      if [ -n "$slack_token" ] && [ -n "$slack_user_id" ]; then
        _env_set "SLACK_BOT_TOKEN" "$slack_token"
        _env_set "SLACK_USER_ID" "$slack_user_id"
        _env_set "PRIMARY_CHANNEL" "slack"
        success "Slack configurado."
      fi
      ;;
    "Pular (configurar depois)")
      warn "Nenhum canal configurado. Configure em .env depois."
      ;;
    *)
      warn "Opção inválida. Configure manualmente em .env depois."
      ;;
  esac
else
  success "Canal já configurado: $PRIMARY_CHANNEL"
fi

# ── 6. Detectar LLM Provider e Gerar Configuração ────────────────
step "Detectando LLM provider..."

ANTHROPIC_KEY=$(_env_get "ANTHROPIC_API_KEY")
OPENAI_KEY=$(_env_get "OPENAI_API_KEY")
GEMINI_KEY=$(_env_get "GEMINI_API_KEY")
GROQ_KEY=$(_env_get "GROQ_API_KEY")
OPENROUTER_KEY=$(_env_get "OPENROUTER_API_KEY")
OPENROUTER_MODEL=$(_env_get "OPENROUTER_MODEL")
PRIMARY_CHANNEL=$(_env_get "PRIMARY_CHANNEL")

if [ -n "$OPENROUTER_KEY" ]; then
  PROVIDER_ID="openrouter"
  PROVIDER_BASE_URL="https://openrouter.ai/api/v1"
  PROVIDER_API_KEY="$OPENROUTER_KEY"
  MODEL_ID="${OPENROUTER_MODEL:-openai/gpt-oss-20b:free}"
  MODEL_NAME="OpenRouter: ${MODEL_ID}"
  PROVIDER_HEADERS='"headers": { "HTTP-Referer": "https://github.com/samurai-py/bastion", "X-Title": "Bastion" },'
  success "Usando OpenRouter ($MODEL_ID)"
elif [ -n "$ANTHROPIC_KEY" ]; then
  PROVIDER_ID="anthropic"
  PROVIDER_BASE_URL="https://api.anthropic.com"
  PROVIDER_API_KEY="$ANTHROPIC_KEY"
  MODEL_ID="claude-sonnet-4-5"
  MODEL_NAME="Claude Sonnet 4.5"
  PROVIDER_HEADERS=""
  success "Usando Anthropic (Claude)"
elif [ -n "$OPENAI_KEY" ]; then
  PROVIDER_ID="openai"
  PROVIDER_BASE_URL="https://api.openai.com/v1"
  PROVIDER_API_KEY="$OPENAI_KEY"
  MODEL_ID="gpt-4o"
  MODEL_NAME="GPT-4o"
  PROVIDER_HEADERS=""
  success "Usando OpenAI (GPT-4o)"
elif [ -n "$GEMINI_KEY" ]; then
  PROVIDER_ID="google-gemini"
  PROVIDER_BASE_URL="https://generativelanguage.googleapis.com/v1beta/openai"
  PROVIDER_API_KEY="$GEMINI_KEY"
  MODEL_ID="gemini-2.0-flash"
  MODEL_NAME="Gemini 2.0 Flash"
  PROVIDER_HEADERS=""
  success "Usando Google Gemini"
elif [ -n "$GROQ_KEY" ]; then
  PROVIDER_ID="groq"
  PROVIDER_BASE_URL="https://api.groq.com/openai/v1"
  PROVIDER_API_KEY="$GROQ_KEY"
  MODEL_ID="llama-3.3-70b-versatile"
  MODEL_NAME="Llama 3.3 70B (Groq)"
  PROVIDER_HEADERS=""
  success "Usando Groq (Llama 3.3)"
else
  error "Nenhuma API key de LLM encontrada. Configure em .env e rode novamente."
  exit 1
fi

# ── 7. Gerar openclaw.json com Configuração Robusta ───────────────
step "Gerando configuração OpenClaw..."

CONFIG_DIR="$INSTALL_DIR/config"
mkdir -p "$CONFIG_DIR"

# Ler configurações de canais do .env
TELEGRAM_BOT_TOKEN=$(_env_get "TELEGRAM_BOT_TOKEN")
TELEGRAM_USER_ID=$(_env_get "TELEGRAM_USER_ID")
DISCORD_BOT_TOKEN=$(_env_get "DISCORD_BOT_TOKEN")
DISCORD_USER_ID=$(_env_get "DISCORD_USER_ID")
SLACK_BOT_TOKEN=$(_env_get "SLACK_BOT_TOKEN")
SLACK_USER_ID=$(_env_get "SLACK_USER_ID")
WA_URL=$(_env_get "WHATSAPP_API_URL")
WA_KEY=$(_env_get "WHATSAPP_API_KEY")
WA_NUMBER=$(_env_get "WHATSAPP_NUMBER")

# Construir seção de channels dinamicamente
CHANNELS_CONFIG=""

if [ -n "$TELEGRAM_BOT_TOKEN" ]; then
  TELEGRAM_ALLOW=""
  [ -n "$TELEGRAM_USER_ID" ] && TELEGRAM_ALLOW=",
      \"allowFrom\": [\"${TELEGRAM_USER_ID}\"]"
  CHANNELS_CONFIG="${CHANNELS_CONFIG}
    \"telegram\": {
      \"enabled\": true${TELEGRAM_ALLOW},
      \"dmPolicy\": \"allowlist\"
    },"
fi

if [ -n "$DISCORD_BOT_TOKEN" ]; then
  DISCORD_ALLOW=""
  [ -n "$DISCORD_USER_ID" ] && DISCORD_ALLOW=",
      \"allowFrom\": [\"${DISCORD_USER_ID}\"]"
  CHANNELS_CONFIG="${CHANNELS_CONFIG}
    \"discord\": {
      \"enabled\": true${DISCORD_ALLOW},
      \"dmPolicy\": \"allowlist\"
    },"
fi

if [ -n "$SLACK_BOT_TOKEN" ]; then
  SLACK_ALLOW=""
  [ -n "$SLACK_USER_ID" ] && SLACK_ALLOW=",
      \"allowFrom\": [\"${SLACK_USER_ID}\"]"
  CHANNELS_CONFIG="${CHANNELS_CONFIG}
    \"slack\": {
      \"enabled\": true${SLACK_ALLOW},
      \"dmPolicy\": \"allowlist\"
    },"
fi

if [ -n "$WA_URL" ] && [ -n "$WA_KEY" ]; then
  CHANNELS_CONFIG="${CHANNELS_CONFIG}
    \"whatsapp\": {
      \"enabled\": true,
      \"apiUrl\": \"${WA_URL}\",
      \"apiKey\": \"${WA_KEY}\",
      \"allowFrom\": [\"${WA_NUMBER}\"],
      \"dmPolicy\": \"allowlist\"
    },"
fi

# Remover última vírgula se houver canais
if [ -n "$CHANNELS_CONFIG" ]; then
  CHANNELS_CONFIG=$(echo "$CHANNELS_CONFIG" | sed 's/,$//')
  CHANNELS_SECTION=",
    \"channels\": {${CHANNELS_CONFIG}
    }"
else
  CHANNELS_SECTION=""
fi

# Gerar openclaw.json com channels integrados
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
    "mode": "local",
    "auth": { "mode": "none" }
  }${CHANNELS_SECTION},
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

success "OpenClaw configurado com ${MODEL_NAME}"

# ── 8. Preparar Workspace e Sincronizar Contexto do Bastion ──────
step "Preparando workspace do Bastion..."

# Criar diretórios necessários
mkdir -p "$INSTALL_DIR/personas" "$INSTALL_DIR/tmp"
chmod 1777 "$INSTALL_DIR/tmp"

# Corrigir permissões do config para o usuário do container
docker run --rm -v "$INSTALL_DIR/config:/data" alpine chown -R 1000:1000 /data 2>/dev/null || true

# Pré-autorizar o user_id no USER.md (respeitando AGENTS.md)
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
<!-- O campo authorized_user_ids é imutável pelo agente — gerenciado apenas pelo installer. -->
EOF
  success "User ID ${USER_ID} pré-autorizado no USER.md."
fi

# Sincronizar arquivos do Bastion para o workspace do OpenClaw
WORKSPACE_DIR="$INSTALL_DIR/config/workspace"
mkdir -p "$WORKSPACE_DIR"

for f in SOUL.md USER.md AGENTS.md HEARTBEAT.md; do
  [ -f "$INSTALL_DIR/$f" ] && cp "$INSTALL_DIR/$f" "$WORKSPACE_DIR/$f"
done

success "Contexto do Bastion sincronizado com OpenClaw."

# ── 9. Iniciar Bastion com Healthcheck ──────────────────────────
step "Iniciando Bastion..."

cd "$INSTALL_DIR"

# Pull da imagem mais recente para evitar cache corrompido
docker compose pull --quiet

# Força recreação para aplicar novas configurações
docker compose up -d --force-recreate --remove-orphans

# Aguardar o container ficar saudável
info "Aguardando o Bastion inicializar..."
sleep 5

if docker ps --filter "name=openclaw" --format "{{.Status}}" | grep -q "Up"; then
  success "Bastion está rodando!"
else
  warn "Container iniciado mas pode estar com problemas. Verifique os logs:"
  echo "  cd $INSTALL_DIR && docker compose logs -f"
fi

# ── 10. Verificação Final ────────────────────────────────────────
step "Verificando instalação..."

VALIDATION_FAILED=false

# Verificar se o .env tem as variáveis necessárias
if [ ! -f "$INSTALL_DIR/.env" ]; then
  error "Arquivo .env não encontrado"
  VALIDATION_FAILED=true
fi

# Verificar se tem pelo menos um LLM configurado
LLM_FOUND=false
for key in OPENROUTER_API_KEY ANTHROPIC_API_KEY OPENAI_API_KEY GEMINI_API_KEY GROQ_API_KEY; do
  val=$(_env_get "$key")
  if [ -n "$val" ]; then
    LLM_FOUND=true
    break
  fi
done

if [ "$LLM_FOUND" = false ]; then
  warn "Nenhum LLM configurado no .env"
  VALIDATION_FAILED=true
fi

# Verificar se tem pelo menos um canal configurado
CHANNEL_FOUND=false
for key in TELEGRAM_BOT_TOKEN DISCORD_BOT_TOKEN SLACK_BOT_TOKEN WHATSAPP_API_URL; do
  val=$(_env_get "$key")
  if [ -n "$val" ]; then
    CHANNEL_FOUND=true
    break
  fi
done

if [ "$CHANNEL_FOUND" = false ]; then
  warn "Nenhum canal configurado no .env"
  VALIDATION_FAILED=true
fi

# Verificar se o openclaw.json foi criado
if [ ! -f "$CONFIG_DIR/openclaw.json" ]; then
  error "Arquivo openclaw.json não foi criado"
  VALIDATION_FAILED=true
else
  # Verificar se tem a seção channels no openclaw.json
  if ! grep -q '"channels"' "$CONFIG_DIR/openclaw.json"; then
    warn "Seção 'channels' não encontrada no openclaw.json"
    VALIDATION_FAILED=true
  fi
  
  # Verificar se tem a seção models
  if ! grep -q '"models"' "$CONFIG_DIR/openclaw.json"; then
    error "Seção 'models' não encontrada no openclaw.json"
    VALIDATION_FAILED=true
  fi
fi

# Verificar se o workspace foi criado
if [ ! -d "$INSTALL_DIR/config/workspace" ]; then
  error "Workspace não foi criado"
  VALIDATION_FAILED=true
else
  # Verificar arquivos essenciais
  for f in SOUL.md USER.md AGENTS.md; do
    if [ ! -f "$INSTALL_DIR/config/workspace/$f" ]; then
      warn "Arquivo $f não encontrado no workspace"
      VALIDATION_FAILED=true
    fi
  done
  
  # Verificar se tem skills
  if [ ! -d "$INSTALL_DIR/config/workspace/skills" ]; then
    warn "Pasta skills não encontrada no workspace"
    VALIDATION_FAILED=true
  fi
fi

# Verificar se o container está rodando
if docker ps --filter "name=openclaw" --format "{{.Status}}" | grep -q "Up"; then
  success "Container OpenClaw está rodando"
  
  # Verificar se o Telegram conectou (se configurado)
  if [ -n "$(_env_get TELEGRAM_BOT_TOKEN)" ]; then
    sleep 3
    if docker compose -f "$INSTALL_DIR/docker-compose.yml" logs openclaw 2>&1 | grep -q "starting provider (@"; then
      success "Telegram conectado com sucesso"
    else
      warn "Telegram pode não ter conectado. Verifique os logs."
    fi
  fi
else
  error "Container OpenClaw não está rodando"
  VALIDATION_FAILED=true
fi

if [ "$VALIDATION_FAILED" = true ]; then
  echo ""
  warn "Algumas verificações falharam. Revise a configuração."
  echo ""
fi

# ── 11. Resumo Final ──────────────────────────────────────────────
step "Instalação concluída!"

echo ""
echo -e "${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
echo -e "  ${GREEN}✓${RESET} Modelo:  ${BOLD}${MODEL_NAME}${RESET}"
[ -n "$PRIMARY_CHANNEL" ] && echo -e "  ${GREEN}✓${RESET} Canal:   ${BOLD}${PRIMARY_CHANNEL}${RESET}"
[ -n "$USER_ID" ] && echo -e "  ${GREEN}✓${RESET} User ID: ${BOLD}${USER_ID}${RESET}"
echo ""
echo -e "  ${CYAN}Próximos passos:${RESET}"
case "$PRIMARY_CHANNEL" in
  telegram) echo -e "    1. Abra o Telegram e envie ${BOLD}/start${RESET} para seu bot" ;;
  whatsapp) echo -e "    1. Envie uma mensagem para seu número WhatsApp" ;;
  discord) echo -e "    1. Envie uma DM para seu bot no Discord" ;;
  slack) echo -e "    1. Envie uma DM para seu bot no Slack" ;;
  *) echo -e "    1. Configure um canal em ${BOLD}.env${RESET} e rode novamente" ;;
esac
echo -e "    2. Complete o onboarding (nome, personas, TOTP)"
echo -e "    3. Comece a usar o Bastion!"
echo ""
echo -e "  ${CYAN}Comandos úteis:${RESET}"
echo -e "    Ver logs:      ${BOLD}cd $INSTALL_DIR && docker compose logs -f${RESET}"
echo -e "    Reiniciar:     ${BOLD}cd $INSTALL_DIR && docker compose restart${RESET}"
echo -e "    Parar:         ${BOLD}cd $INSTALL_DIR && docker compose down${RESET}"
echo -e "    Reconfigurar:  ${BOLD}bash $INSTALL_DIR/installer.sh${RESET}"
echo -e "${BOLD}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
echo ""
