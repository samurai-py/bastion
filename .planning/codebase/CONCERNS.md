---
title: Concerns & Tech Debt
last_mapped: 2026-05-10
---

# Concerns & Technical Debt

## High Priority

### Heavy Optional Dependencies (memupalace)
- `chromadb`, `onnxruntime`, `numpy` are large dependencies (500MB+ combined)
- Optional install via `pip install bastion[memupalace]` but not isolated from base image
- Risk: Docker image bloat if included by default; onboarding friction if excluded
- **File**: `pyproject.toml` optional-dependencies `memupalace` group

### Supabase Adapter Coverage
- `skills/life-log/db/supabase_adapter.py` exists but primary tests use SQLite
- Contract between supabase adapter and `LifeLogProtocol` may drift
- **File**: `skills/life-log/db/supabase_adapter.py`

### Single Integration Test
- Only `tests/test-installer.sh` tests full deployment — no CI/CD pipeline detected
- Installer breakage may go undetected between releases

## Medium Priority

### Skills Without Full Test Coverage
Most skills have a `tests/` directory but test depth varies:
- `skills/mobile-connect/tests/` — contents unknown
- `skills/onboarding/tests/` — onboarding flows not property-tested
- `skills/self-improving/tests/` — AI self-reflection hard to unit test
- `skills/skill-writer/tests/` — generative skill tests unclear
- `skills/output-validator/tests/` — validator coverage unknown
- `skills/weekly-review/tests/` — synthesis logic coverage unknown

### i18n Bootstrapping via sys.path Mutation
Each `i18n.py` mutates `sys.path` at import time to reach `utils/i18n`. This is fragile under certain import orders.
```python
_skills_dir = Path(__file__).resolve().parent.parent
if str(_skills_dir) not in sys.path:
    sys.path.insert(0, str(_skills_dir))
```
- **Files**: All skill `i18n.py` files

### Proactive Engine Architecture
- `skills/proactive-engine/main.py` serves as CLI entrypoint but integration with OpenClaw scheduling is via HEARTBEAT.md
- The boundary between proactive-engine and OpenClaw's native scheduling is not clearly documented

## Low Priority

### No Lock File for Python
- `pyproject.toml` uses version ranges (e.g., `>=2.9`) without a lockfile
- Reproducible builds rely on Docker layer caching, not pinned deps
- Consider `pip-tools` or `uv` for pinned requirements

### TODOs in Codebase
Grep findings (from source):
```
skills/memupalace/ — various inline notes
skills/proactive-engine/ — implementation stubs
```

### STRATEGY.md Not Tracked
- `STRATEGY.md` is present at root but untracked (in `.gitignore` or untracked)
- Strategic context lives outside version control

## Security Notes

### Secrets in .env
- `TOTP_SECRET`, `JWT_SECRET`, `COMPOSIO_CONSUMER_KEY`, `SUPABASE_KEY` all in `.env`
- `.env.example` correctly leaves values empty
- Risk: `.env` accidentally committed (standard risk, mitigated by `.gitignore`)

### Anti-Injection Guardrail
- `skills/guardrails/guardrails.py` implements prompt injection defense (Req 11.3)
- External content treated as data — this is correct but relies on the guardrail being invoked

### TOTP Session Limits
- `TOTP_SESSION_HOURS` and `TOTP_MAX_FAILS` configurable via `.env`
- Default values not hardened — installer sets these

## Performance Notes

- `sqlite-vec` vector search performance degrades with large datasets (no indexing tuning noted)
- ChromaDB (memupalace) may be slow without GPU acceleration (`onnxruntime` CPU-only by default)
- OpenClaw ports bound to `127.0.0.1` only — no external exposure by default (good)
