---
title: Testing
last_mapped: 2026-05-10
---

# Testing

## Framework

- **pytest** ≥8.0 with `asyncio_mode = "auto"` (all async tests auto-detected)
- **Hypothesis** ≥6.100 for property-based testing (PBT) — the primary test strategy
- **pytest-asyncio** ≥0.23 for async skill logic
- **fast-check** (TypeScript mobile plugin tests)

## Test Discovery

pytest configured in `pyproject.toml`:
```toml
[tool.pytest.ini_options]
addopts = "-q"
asyncio_mode = "auto"
testpaths = ["skills"]
python_files = ["test_*.py"]
python_classes = ["Test*"]
python_functions = ["test_*"]
```

Each skill's directory is added to `pythonpath` so tests can import skill modules directly.

## Structure

Tests live inside each skill:
```
skills/
├── crisis-mode/tests/test_crisis_properties.py
├── guardrails/tests/test_guardrail_properties.py
├── life-log/tests/
│   ├── conftest.py
│   ├── life_log_helpers.py
│   └── test_life_log_properties.py
├── bastion-calendar/tests/
│   ├── conftest.py
│   ├── test_composio_contract.py
│   └── test_parser.py
└── [other skills]/tests/
```

## Test Strategy

### Property-Based Testing (primary)
Hypothesis generates random inputs to find edge cases. Naming convention: `test_*_properties.py`.

```python
# Example pattern (crisis-mode)
from hypothesis import given, strategies as st

@given(st.text())
def test_detect_crisis_always_returns_result(message):
    result = detect_crisis(message)
    assert isinstance(result, CrisisResult)
```

### Contract Testing
`bastion-calendar/tests/test_composio_contract.py` validates the Composio API contract — tests that the external integration behaves as expected.

### Unit Tests
Standard pytest tests for deterministic logic (parser, models, etc.).

### Integration / Deployment
`tests/test-installer.sh` — shell script that validates the full installation flow.

## Running Tests

```bash
# All tests
pytest

# Single skill
pytest skills/crisis-mode/

# With coverage (not configured by default)
pytest --cov=skills
```

## Mocking Strategy

- Skills use Protocol interfaces so tests can inject in-memory fakes
- No monkeypatching of concrete adapters — pass a fake that satisfies the Protocol
- External APIs (Composio, LLM) tested via contract tests against real endpoints (requires API keys)

## Coverage

No coverage enforcement configured in CI. Tests focus on behavioral correctness via property testing rather than line coverage metrics.

## Gaps

- Most skills have `tests/` dirs but `__init__.py` / test files may be stubs
- No integration test harness for OpenClaw ↔ skill invocation
- LLM behavior untested (relies on DeepEval / manual validation per CLAUDE.md)
