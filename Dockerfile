# ── STAGE 1: Preparador de Assets ─────────────────────────────────────
FROM alpine:latest AS builder

# Criar estrutura de diretórios do workspace do OpenClaw
RUN mkdir -p /assets/workspace/app/skills \
             /assets/workspace/app/personas \
             /assets/workspace/app/config

# Copiar os arquivos locais (precisam estar no contexto do build)
COPY ./skills /assets/workspace/app/skills
COPY ./personas /assets/workspace/app/personas
COPY SOUL.md USER.md AGENTS.md HEARTBEAT.md /assets/workspace/app/

# Limpeza de arquivos temporários do desenvolvedor (opcional)
RUN find /assets -name "__pycache__" -type d -exec rm -rf {} +
RUN find /assets -name "*.pyc" -delete

# ── STAGE 2: Imagem Final de Runtime ──────────────────────────────────
FROM ghcr.io/openclaw/openclaw:latest

USER root

# Copiar do builder para a imagem final com as permissões corretas
# O OpenClaw espera o workspace em /home/node/.openclaw/workspace
COPY --from=builder --chown=node:node /assets/workspace /home/node/.openclaw/workspace

# Garantir que o diretório persistente de config também exista
RUN mkdir -p /home/node/.openclaw/config && chown -R node:node /home/node/.openclaw

USER node

# Definir o diretório de trabalho padrão do Bastion
WORKDIR /home/node/.openclaw/workspace/app
