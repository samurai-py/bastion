# 🏰 Bastion

Seu assistente pessoal de IA, rodando 100% no seu computador ou servidor. O Bastion aprende como você trabalha, se adapta às diferentes áreas da sua vida e te ajuda a manter o foco no que importa — sem compartilhar seus dados com ninguém.

## O que é?

Agente de IA self-hosted construído sobre o [OpenClaw](https://openclaw.ai). Funciona com **personas** — perfis de comportamento para cada área da sua vida (trabalho, estudos, projetos pessoais). O Bastion detecta automaticamente qual persona usar com base no contexto.

Seus dados ficam 100% com você. Nada vai para servidores externos além das chamadas ao LLM que você escolher.

---

## Instalação

### TL;DR

```bash
bash <(curl -fsSL https://bastion.run/install)
```

Siga o wizard e pronto. Leva 5 minutos.

### Pré-requisitos

- **Docker** ([instalar](https://docs.docker.com/get-docker/))
- **API key de LLM** (pelo menos uma):
  - [OpenRouter](https://openrouter.ai/keys) — recomendado, tem modelos gratuitos
  - [Groq](https://console.groq.com) — gratuito, rápido
  - [Google Gemini](https://aistudio.google.com/app/apikey) — gratuito
  - [Anthropic](https://console.anthropic.com) — pago, melhor qualidade
  - [OpenAI](https://platform.openai.com/api-keys) — pago, popular
- **Canal de mensagens** (pelo menos um):
  - Bot do Telegram (via [@BotFather](https://t.me/BotFather))
  - Evolution API para WhatsApp
  - Bot do Discord
  - App do Slack

### Instalação Interativa

```bash
bash <(curl -fsSL https://bastion.run/install)
```

O instalador vai perguntar:
1. Qual LLM usar (recomendamos OpenRouter com modelos gratuitos)
2. Qual canal configurar (Telegram é o mais fácil)
3. Suas credenciais (API keys, tokens)

Depois disso, ele:
- Verifica/instala Docker se necessário
- Gera todas as configurações automaticamente
- Inicia o Bastion

### Instalação Automatizada (CI/CD)

```bash
export BASTION_WIZARD=false
export OPENROUTER_API_KEY="sk-or-v1-..."
export OPENROUTER_MODEL="openai/gpt-oss-20b:free"
export TELEGRAM_BOT_TOKEN="123456:ABC..."
export TELEGRAM_USER_ID="987654321"

bash <(curl -fsSL https://bastion.run/install)
```

Veja todas as variáveis suportadas em [docs/installer-guide.md](docs/installer-guide.md).

### Instalação Manual

Se preferir configurar na mão:

```bash
git clone https://github.com/samurai-py/bastion.git
cd bastion
cp .env.example .env
nano .env  # preencha suas chaves
docker compose up -d
```

---

## Primeiros Passos

1. Envie `/start` para seu bot no canal configurado
2. Complete o onboarding (nome, personas, TOTP)
3. Comece a usar!

O Bastion vai criar suas personas automaticamente com base no que você faz. Depois disso, é só conversar normalmente — ele detecta o contexto e responde com a persona certa.

---

## Troubleshooting

### Docker não encontrado

O instalador oferece instalação automática. Se recusar:
- **Linux:** `curl -fsSL https://get.docker.com | sh`
- **macOS/Windows:** [Docker Desktop](https://docs.docker.com/get-docker/)

### Bot não responde

```bash
cd ~/bastion
docker compose logs -f
```

Verifique se:
- O container está rodando: `docker ps`
- Seu user_id está correto: `grep authorized_user_ids USER.md`
- Para Telegram, obtenha seu ID com [@userinfobot](https://t.me/userinfobot)

### Reconfigurar do zero

```bash
cd ~/bastion
docker compose down -v
rm -rf config/
bash installer.sh
```

---

## Documentação

- [Guia do Instalador](docs/installer-guide.md) — referência técnica completa
- [VPS Setup](docs/vps-setup.md) — subir numa VPS do zero
- [Segurança](docs/security.md) — guardrails e autenticação
- [Personas](docs/personas.md) — como funcionam
- [Modo Crise](docs/crisis-mode.md) — replanejamento automático
- [FAQ](docs/faq.md) — perguntas frequentes

---

## Licença

MIT
