# 🏰 Bastion

Seu assistente pessoal de IA, rodando 100% no seu computador ou servidor. O Bastion aprende como você trabalha, se adapta às diferentes áreas da sua vida e te ajuda a manter o foco no que importa — sem compartilhar seus dados com ninguém.

---

## O que é o Bastion?

O Bastion é um agente de IA self-hosted construído sobre o [OpenClaw](https://openclaw.ai). Você instala uma vez, configura suas chaves de API, e ele fica disponível no seu Telegram ou WhatsApp — como um assistente particular que só você acessa.

Ele funciona com **personas**: perfis de comportamento para cada área da sua vida. Você pode ter uma persona para trabalho, outra para estudos, outra para projetos pessoais. O Bastion detecta automaticamente qual persona usar com base no contexto da conversa.

Seus dados ficam 100% com você. Nada vai para servidores externos além das chamadas ao LLM que você escolher.

---

## Pré-requisitos

Antes de instalar, você vai precisar de:

- **Docker + Docker Compose** — para rodar o Bastion ([instalar Docker](https://docs.docker.com/get-docker/))
- **API key de pelo menos 1 LLM** — Anthropic (Claude), OpenAI (GPT), Google (Gemini) ou Groq
- **Conta Maton gratuita** — crie em [maton.ai](https://maton.ai) e gere uma API key (é grátis)
- **Canal de mensagens** — token do seu bot no Telegram **ou** conta Twilio para WhatsApp

---

## Instalação em 3 passos

### Passo 1 — Rode o instalador

```bash
curl -fsSL https://bastion.run/install | bash
```

O instalador verifica se o Docker está instalado, baixa os arquivos do Bastion e cria o arquivo `.env` para você preencher.

### Passo 2 — Preencha o `.env`

Abra o arquivo `.env` que foi criado na pasta `bastion/` e preencha suas chaves:

```env
# LLM — preencha pelo menos uma
ANTHROPIC_API_KEY=sk-ant-...
OPENAI_API_KEY=sk-...
GEMINI_API_KEY=...
GROQ_API_KEY=...

# Maton (obrigatório)
MATON_API_KEY=...

# Canal de mensagens — escolha um
TELEGRAM_BOT_TOKEN=...
# ou
TWILIO_ACCOUNT_SID=...
TWILIO_AUTH_TOKEN=...
TWILIO_WHATSAPP_NUMBER=...
```

### Passo 3 — Suba o Bastion

```bash
cd bastion
docker compose up -d
```

Pronto. O Bastion está rodando.

---

## Primeiros passos

Abra o Telegram (ou WhatsApp) e envie `/start` para o seu bot.

O Bastion vai te guiar por um onboarding de alguns minutos:

1. Vai perguntar seu nome e o que você faz
2. Vai criar personas para cada área da sua vida que você informar
3. Vai configurar autenticação por código TOTP (via Authy) para proteger seu acesso
4. No final, vai mostrar um resumo das suas personas e você já pode começar a usar

A partir daí, é só conversar normalmente. O Bastion detecta o contexto e usa a persona certa automaticamente.

---

## Documentação

- [Instalação local — passo a passo](docs/getting-started.md)
- [Subindo numa VPS do zero](docs/vps-setup.md)
- [Guia de segurança](docs/security.md)
- [Conectar o app mobile](docs/connect-app.md)
- [Personas — o que são e como criar](docs/personas.md)
- [Modo Crise — replanejamento automático de agenda](docs/crisis-mode.md)
- [Perguntas frequentes](docs/faq.md)

---

## Licença

MIT
