# ── Bastion Runtime Image ────────────────────────────────────────────
# Skills, personas e arquivos core vêm via bind mount (docker-compose.yml).
# Este Dockerfile só instala as dependências Python necessárias.
# ────────────────────────────────────────────────────────────────────
FROM ghcr.io/openclaw/openclaw:latest

# Instalar Python + pip (Debian-based)
USER root
RUN apt-get update && apt-get install -y python3-pip && rm -rf /var/lib/apt/lists/*

# Instalar apenas as dependências Python (não o pacote bastion em si)
RUN pip3 install --no-cache-dir --break-system-packages \
    "pyotp>=2.9" "PyJWT>=2.8" "httpx>=0.27" "sqlite-vec>=0.1" "pydantic>=2.7" "qrcode>=7.4"

USER node
