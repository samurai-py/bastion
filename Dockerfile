# ── STAGE 1: Preparador de Assets ─────────────────────────────────────
FROM alpine:latest AS builder

# Criar estrutura de diretórios do workspace do OpenClaw
RUN mkdir -p /assets/workspace/app/skills \
             /assets/workspace/app/personas \
             /assets/workspace/app/config

# Copiar os arquivos locais
COPY ./skills /assets/workspace/app/skills
COPY ./personas /assets/workspace/app/personas
COPY SOUL.md USER.md AGENTS.md HEARTBEAT.md README.md pyproject.toml /assets/workspace/app/

# Limpeza de arquivos temporários
RUN find /assets -name "__pycache__" -type d -exec rm -rf {} +
RUN find /assets -name "*.pyc" -delete

# ── STAGE 2: Imagem Final de Runtime ──────────────────────────────────
FROM ghcr.io/openclaw/openclaw:latest

# Instalar o suporte ao Python no container (Debian-based) + Pip
USER root
RUN apt-get update && apt-get install -y python3-pip && rm -rf /var/lib/apt/lists/*

# Copiar os assets do builder para o local correto do OpenClaw
COPY --from=builder --chown=node:node /assets/workspace/app/ /home/node/.openclaw/workspace/app/

# Instalar dependências globais de Python declaradas no pyproject.toml
# Rodamos pip apontando para o diretório sem mudar o WORKDIR global
RUN pip3 install --no-cache-dir /home/node/.openclaw/workspace/app/ --break-system-packages

# Ajustar as permissões finais
RUN chown -R node:node /home/node/.openclaw

USER node
# NOTA: Não alteramos o WORKDIR aqui para não quebrar o entrypoint original da imagem base
