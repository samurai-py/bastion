---
title: Code Conventions
last_mapped: 2026-05-10
---

# Code Conventions

## Code Style

- **Formatter/Linter**: Ruff (configured in `pyproject.toml` under `[tool.ruff]`)
- **Type checker**: Pyright in `standard` mode, Python 3.12
- **Future annotations**: All files use `from __future__ import annotations` at top

## Architecture Patterns

### Hexagonal Architecture (enforced per-skill)
Every skill with persistence follows this structure:
1. **Domain models** — `@dataclass` objects, pure data
2. **Protocol port** — `class XPersistenceProtocol(Protocol): ...` using `@runtime_checkable`
3. **Adapter** — concrete implementation (e.g., `sqlite_adapter.py`, `supabase_adapter.py`)
4. **Factory** — creates the correct adapter from `Settings.from_env()`
5. **Logic functions** — accept the protocol type, not the concrete adapter

```python
# Port pattern used in life-log, weight-system, persona-engine
from typing import Protocol, runtime_checkable

@runtime_checkable
class WeightPersistenceProtocol(Protocol):
    def get_current_weight(self, slug: str) -> float: ...
    def set_current_weight(self, slug: str, weight: float) -> None: ...
```

### Settings Pattern
Config read from environment via a `Settings` dataclass:
```python
@dataclass
class Settings:
    DB_STRATEGY: str = "sqlite"
    SQLITE_PATH: str = "db/life-log.db"

    @classmethod
    def from_env(cls) -> "Settings":
        return cls(DB_STRATEGY=os.getenv("DB_STRATEGY", "sqlite"), ...)
```

### i18n Pattern
Each skill has an `i18n.py` that re-exports from `skills/utils/i18n`:
```python
# skills/crisis-mode/i18n.py
from utils.i18n import get_string, load_locale
__all__ = ["get_string", "load_locale"]
```

Skills add `skills/` parent dir to `sys.path` to resolve `utils.i18n`.

## Naming

| Item | Convention | Example |
|------|-----------|---------|
| Skill directories | `kebab-case` | `crisis-mode`, `life-log` |
| Python modules | `snake_case` | `crisis_mode.py` |
| Classes | `PascalCase` | `CrisisResult`, `GuardrailEngine` |
| Dataclasses | `PascalCase` | `Persona`, `ActivePersona` |
| Protocol classes | `*Protocol` suffix | `WeightPersistenceProtocol` |
| Functions | `snake_case` | `detect_crisis()`, `match_personas()` |
| Constants | `UPPER_SNAKE` | `MIN_DEEP_WORK_HOURS` |

## Error Handling

- Python logging module used throughout (`logger = logging.getLogger(__name__)`)
- Dataclasses with result types used instead of raising exceptions where possible (e.g., `GuardrailResult`, `CrisisResult`)
- Fallback pattern: algorithms return `fallback=True` with options rather than raising on insufficient data

## Docstrings

Module-level docstrings document the full behavior contract:
- Requirements mapped (e.g., `Req 11.1`)
- Algorithm steps listed
- Out-of-scope items explicitly noted

Function-level docstrings are brief (1-2 sentences).

## Import Order

Standard `isort`/Ruff-compatible:
1. `from __future__ import annotations`
2. stdlib
3. third-party
4. local (relative imports)
