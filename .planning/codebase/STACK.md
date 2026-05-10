---
title: Tech Stack
last_mapped: 2026-05-10
---

# Tech Stack

## Languages

| Language | Version | Role |
|----------|---------|------|
| Python | 3.12 | All skills logic |
| TypeScript | — | Mobile connect plugin (Express) |
| Shell | — | Installer, test-installer.sh |

## Runtime

| Component | Version | Role |
|-----------|---------|------|
| OpenClaw | Node.js-based | AI agent runtime (core engine) |
| Docker | latest | Container orchestration |
| Caddy | alpine | Reverse proxy / HTTPS termination |

## Python Dependencies

### Core (always installed)
| Package | Version | Purpose |
|---------|---------|---------|
| `pyotp` | >=2.9 | TOTP authentication |
| `PyJWT` | >=2.8 | JWT signing for mobile connect |
| `httpx` | >=0.27 | Async HTTP client |
| `sqlite-vec` | >=0.1 | Vector search extension for SQLite |
| `pydantic` | >=2.7 | Data validation and models |
| `qrcode` | >=7.4 | QR code generation for TOTP setup |

### Optional: Supabase
| Package | Version | Purpose |
|---------|---------|---------|
| `supabase` | >=2.4 | Cloud DB alternative to SQLite |

### Optional: Memupalace (semantic memory)
| Package | Version | Purpose |
|---------|---------|---------|
| `chromadb` | >=0.5,<0.7 | Vector store for embeddings |
| `onnxruntime` | >=1.17 | ONNX model inference |
| `numpy` | >=1.26 | Numerical operations |

### Dev
| Package | Version | Purpose |
|---------|---------|---------|
| `pytest` | >=8.0 | Test framework |
| `hypothesis` | >=6.100 | Property-based testing |
| `ruff` | >=0.4 | Linting and formatting |
| `pyright` | >=1.1 | Static type checking |
| `pytest-asyncio` | >=0.23 | Async test support |

## Configuration

- `pyproject.toml` — Python project config, deps, tool settings (ruff, pyright, pytest)
- `docker-compose.yml` — Service definitions (openclaw, caddy, optional ollama)
- `Dockerfile` — Python deps installer image
- `Caddyfile` — Reverse proxy rules
- `.env` (from `.env.example`) — Runtime secrets and config

## Tooling

| Tool | Config | Purpose |
|------|--------|---------|
| Ruff | `[tool.ruff]` in pyproject.toml | Lint + format |
| Pyright | `[tool.pyright]` in pyproject.toml | Type checking (`standard` mode) |
| pytest | `[tool.pytest.ini_options]` in pyproject.toml | Test runner |
