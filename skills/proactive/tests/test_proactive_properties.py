"""
Property-based tests for the Proactive skill.

**Validates: Requirements 7.2, 7.5**

Properties tested:
  - Property 17: Sugestão de retomada é gerada para personas inativas há ≥ 3 dias
  - Property 18: Alerta de CVE é gerado para qualquer skill comprometida
"""

from __future__ import annotations

import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any

import pytest
from hypothesis import given, settings
from hypothesis import strategies as st

# Allow importing proactive from the parent directory
sys.path.insert(0, str(Path(__file__).parent.parent))

from proactive import (
    CVEAlert,
    ClawHubClient,
    LifeLogAdapter,
    PersonaActivity,
    check_cve_alerts,
    check_inactive_personas,
)

# ---------------------------------------------------------------------------
# Fake adapters (no mocks — real implementations of the Protocols)
# ---------------------------------------------------------------------------


class FakeLifeLogAdapter:
    """In-memory LifeLogAdapter for testing."""

    def __init__(self, last_interactions: dict[str, datetime | None]) -> None:
        self._data = last_interactions

    async def get_last_interaction(self, persona_slug: str) -> datetime | None:
        return self._data.get(persona_slug)


class FakeClawHubClient:
    """In-memory ClawHubClient for testing."""

    def __init__(self, cves_by_skill: dict[str, list[dict[str, str]]]) -> None:
        self._data = cves_by_skill

    async def get_cves(self, skill_name: str) -> list[dict[str, str]]:
        return self._data.get(skill_name, [])

    async def get_batch_cves(self, skill_names: list[str]) -> dict[str, list[dict[str, str]]]:
        return {name: self._data.get(name, []) for name in skill_names if name in self._data}


# ---------------------------------------------------------------------------
# Hypothesis strategies
# ---------------------------------------------------------------------------

_slug = st.text(
    alphabet="abcdefghijklmnopqrstuvwxyz0123456789-",
    min_size=1,
    max_size=30,
).filter(lambda s: not s.startswith("-") and not s.endswith("-"))

_skill_name = st.text(
    alphabet="abcdefghijklmnopqrstuvwxyz0123456789-/",
    min_size=1,
    max_size=50,
).filter(lambda s: len(s) > 0)

_cve_id = st.from_regex(r"CVE-[0-9]{4}-[0-9]{4,7}", fullmatch=True)

_severity = st.sampled_from(["CRITICAL", "HIGH", "MEDIUM", "LOW"])

_description = st.text(min_size=1, max_size=200).filter(lambda s: "\n" not in s)

_days_inactive = st.integers(min_value=3, max_value=365)

_days_active = st.integers(min_value=0, max_value=2)

_threshold = st.integers(min_value=1, max_value=7)


def _past_datetime(days_ago: int) -> datetime:
    """Return a timezone-aware datetime N days in the past."""
    return datetime.now(tz=timezone.utc) - timedelta(days=days_ago)


# ---------------------------------------------------------------------------
# Property 17 — Sugestão de retomada é gerada para personas inativas há ≥ 3 dias
# Validates: Requirements 7.2
# ---------------------------------------------------------------------------


@given(
    slug=_slug,
    days_ago=_days_inactive,
)
@settings(max_examples=200)
def test_property17_inactive_persona_is_detected(
    slug: str,
    days_ago: int,
) -> None:
    """
    **Property 17: Sugestão de retomada é gerada para personas inativas há ≥ 3 dias**

    For any persona whose last interaction was >= 3 days ago, check_inactive_personas()
    must include that persona in the result.

    **Validates: Requirements 7.2**
    """
    import asyncio

    last = _past_datetime(days_ago)
    adapter = FakeLifeLogAdapter({slug: last})

    result = asyncio.run(check_inactive_personas([slug], adapter, threshold_days=3))

    slugs_in_result = [p.persona_slug for p in result]
    assert slug in slugs_in_result, (
        f"Persona {slug!r} inactive for {days_ago} days must appear in result "
        f"(threshold=3 days). Got: {slugs_in_result}"
    )


@given(
    slug=_slug,
    days_ago=_days_active,
)
@settings(max_examples=200)
def test_property17_active_persona_is_not_flagged(
    slug: str,
    days_ago: int,
) -> None:
    """
    **Property 17 (active variant): Persona ativa recentemente não é incluída**

    For any persona whose last interaction was < 3 days ago, check_inactive_personas()
    must NOT include that persona in the result.

    **Validates: Requirements 7.2**
    """
    import asyncio

    last = _past_datetime(days_ago)
    adapter = FakeLifeLogAdapter({slug: last})

    result = asyncio.run(check_inactive_personas([slug], adapter, threshold_days=3))

    slugs_in_result = [p.persona_slug for p in result]
    assert slug not in slugs_in_result, (
        f"Persona {slug!r} active {days_ago} days ago must NOT appear in result "
        f"(threshold=3 days). Got: {slugs_in_result}"
    )


@given(slug=_slug)
@settings(max_examples=100)
def test_property17_persona_with_no_history_is_flagged(slug: str) -> None:
    """
    **Property 17 (no-history variant): Persona sem histórico é considerada inativa**

    A persona with no interaction history (last_interaction=None) must be
    included in the inactive list.

    **Validates: Requirements 7.2**
    """
    import asyncio

    adapter = FakeLifeLogAdapter({slug: None})

    result = asyncio.run(check_inactive_personas([slug], adapter, threshold_days=3))

    slugs_in_result = [p.persona_slug for p in result]
    assert slug in slugs_in_result, (
        f"Persona {slug!r} with no history must appear in inactive result"
    )


@given(
    slugs=st.lists(_slug, min_size=2, max_size=10, unique=True),
    threshold=_threshold,
)
@settings(max_examples=100)
def test_property17_only_inactive_personas_returned(
    slugs: list[str],
    threshold: int,
) -> None:
    """
    **Property 17 (mixed variant): Apenas personas inativas são retornadas**

    When a mix of active and inactive personas is provided, only the inactive
    ones (days_inactive >= threshold) must appear in the result.

    **Validates: Requirements 7.2**
    """
    import asyncio

    # Split slugs into inactive (days >= threshold) and active (days < threshold)
    half = len(slugs) // 2
    inactive_slugs = slugs[:half]
    active_slugs = slugs[half:]

    last_interactions: dict[str, datetime | None] = {}
    for s in inactive_slugs:
        last_interactions[s] = _past_datetime(threshold + 1)
    for s in active_slugs:
        last_interactions[s] = _past_datetime(max(0, threshold - 1))

    adapter = FakeLifeLogAdapter(last_interactions)
    result = asyncio.run(check_inactive_personas(slugs, adapter, threshold_days=threshold))

    result_slugs = {p.persona_slug for p in result}

    for s in inactive_slugs:
        assert s in result_slugs, (
            f"Inactive persona {s!r} must be in result (threshold={threshold})"
        )
    for s in active_slugs:
        assert s not in result_slugs, (
            f"Active persona {s!r} must NOT be in result (threshold={threshold})"
        )


@given(
    slug=_slug,
    days_ago=_days_inactive,
)
@settings(max_examples=100)
def test_property17_days_inactive_value_is_correct(
    slug: str,
    days_ago: int,
) -> None:
    """
    **Property 17 (days_inactive value): O campo days_inactive reflete os dias reais**

    The days_inactive field in PersonaActivity must match the actual number of
    days since the last interaction.

    **Validates: Requirements 7.2**
    """
    import asyncio

    last = _past_datetime(days_ago)
    adapter = FakeLifeLogAdapter({slug: last})

    result = asyncio.run(check_inactive_personas([slug], adapter, threshold_days=3))

    assert len(result) == 1
    activity = result[0]
    # Allow ±1 day tolerance due to floating-point time arithmetic
    assert abs(activity.days_inactive - days_ago) <= 1, (
        f"days_inactive={activity.days_inactive} should be ~{days_ago} "
        f"for last_interaction={last.isoformat()}"
    )


@given(
    slugs=st.lists(_slug, min_size=2, max_size=10, unique=True),
)
@settings(max_examples=100)
def test_property17_result_sorted_by_days_inactive_descending(
    slugs: list[str],
) -> None:
    """
    **Property 17 (sort order): Resultado ordenado por dias de inatividade decrescente**

    The result of check_inactive_personas() must be sorted by days_inactive
    descending (most inactive persona first).

    **Validates: Requirements 7.2**
    """
    import asyncio

    # All personas inactive, with varying days
    last_interactions = {
        s: _past_datetime(3 + i) for i, s in enumerate(slugs)
    }
    adapter = FakeLifeLogAdapter(last_interactions)

    result = asyncio.run(check_inactive_personas(slugs, adapter, threshold_days=3))

    for i in range(len(result) - 1):
        assert result[i].days_inactive >= result[i + 1].days_inactive, (
            f"Result not sorted: result[{i}].days_inactive={result[i].days_inactive} "
            f"< result[{i+1}].days_inactive={result[i+1].days_inactive}"
        )


# ---------------------------------------------------------------------------
# Property 18 — Alerta de CVE é gerado para qualquer skill comprometida
# Validates: Requirements 7.5
# ---------------------------------------------------------------------------


@given(
    skill_name=_skill_name,
    cve_id=_cve_id,
    severity=_severity,
    description=_description,
)
@settings(max_examples=200)
def test_property18_cve_alert_generated_for_compromised_skill(
    skill_name: str,
    cve_id: str,
    severity: str,
    description: str,
) -> None:
    """
    **Property 18: Alerta de CVE é gerado para qualquer skill comprometida**

    For any installed skill with a CVE detected via ClawHub API, check_cve_alerts()
    must return a CVEAlert for that skill.

    **Validates: Requirements 7.5**
    """
    import asyncio

    cve_record = {"cve_id": cve_id, "severity": severity, "description": description}
    client = FakeClawHubClient({skill_name: [cve_record]})

    result = asyncio.run(check_cve_alerts([skill_name], client))

    assert len(result) >= 1, (
        f"Expected at least 1 CVEAlert for skill {skill_name!r} with CVE {cve_id!r}, "
        f"got {len(result)}"
    )
    alert = result[0]
    assert alert.skill_name == skill_name
    assert alert.cve_id == cve_id
    assert alert.severity == severity
    assert alert.description == description


@given(skill_name=_skill_name)
@settings(max_examples=200)
def test_property18_no_alert_for_clean_skill(skill_name: str) -> None:
    """
    **Property 18 (clean variant): Nenhum alerta para skill sem CVEs**

    For any installed skill with no CVEs, check_cve_alerts() must return
    an empty list.

    **Validates: Requirements 7.5**
    """
    import asyncio

    client = FakeClawHubClient({skill_name: []})

    result = asyncio.run(check_cve_alerts([skill_name], client))

    assert result == [], (
        f"Expected no CVEAlerts for clean skill {skill_name!r}, got {result}"
    )


@given(
    skill_names=st.lists(_skill_name, min_size=1, max_size=10, unique=True),
    cve_id=_cve_id,
    severity=_severity,
    description=_description,
)
@settings(max_examples=100)
def test_property18_all_compromised_skills_generate_alerts(
    skill_names: list[str],
    cve_id: str,
    severity: str,
    description: str,
) -> None:
    """
    **Property 18 (multiple skills): Todas as skills comprometidas geram alertas**

    When multiple installed skills all have CVEs, check_cve_alerts() must
    return at least one CVEAlert per compromised skill.

    **Validates: Requirements 7.5**
    """
    import asyncio

    cve_record = {"cve_id": cve_id, "severity": severity, "description": description}
    cves_by_skill = {name: [cve_record] for name in skill_names}
    client = FakeClawHubClient(cves_by_skill)

    result = asyncio.run(check_cve_alerts(skill_names, client))

    alerted_skills = {a.skill_name for a in result}
    for name in skill_names:
        assert name in alerted_skills, (
            f"Compromised skill {name!r} must have a CVEAlert in result"
        )


@given(
    skill_name=_skill_name,
    cves=st.lists(
        st.builds(
            dict,
            cve_id=_cve_id,
            severity=_severity,
            description=_description,
        ),
        min_size=2,
        max_size=5,
    ),
)
@settings(max_examples=100)
def test_property18_multiple_cves_per_skill_all_reported(
    skill_name: str,
    cves: list[dict[str, str]],
) -> None:
    """
    **Property 18 (multiple CVEs): Todos os CVEs de uma skill são reportados**

    When a single skill has multiple CVEs, check_cve_alerts() must return
    one CVEAlert per CVE — none must be silently dropped.

    **Validates: Requirements 7.5**
    """
    import asyncio

    client = FakeClawHubClient({skill_name: cves})

    result = asyncio.run(check_cve_alerts([skill_name], client))

    assert len(result) == len(cves), (
        f"Expected {len(cves)} CVEAlerts for skill {skill_name!r} with {len(cves)} CVEs, "
        f"got {len(result)}"
    )


@given(
    skill_names=st.lists(_skill_name, min_size=2, max_size=8, unique=True),
    cve_id=_cve_id,
    severity=_severity,
    description=_description,
)
@settings(max_examples=100)
def test_property18_clean_skills_do_not_generate_alerts(
    skill_names: list[str],
    cve_id: str,
    severity: str,
    description: str,
) -> None:
    """
    **Property 18 (mixed skills): Skills limpas não geram alertas mesmo com outras comprometidas**

    When some skills are clean and others are compromised, only the compromised
    ones must appear in the result.

    **Validates: Requirements 7.5**
    """
    import asyncio

    half = len(skill_names) // 2
    compromised = skill_names[:half]
    clean = skill_names[half:]

    cve_record = {"cve_id": cve_id, "severity": severity, "description": description}
    cves_by_skill: dict[str, list[dict[str, str]]] = {}
    for name in compromised:
        cves_by_skill[name] = [cve_record]
    for name in clean:
        cves_by_skill[name] = []

    client = FakeClawHubClient(cves_by_skill)
    result = asyncio.run(check_cve_alerts(skill_names, client))

    alerted_skills = {a.skill_name for a in result}

    for name in compromised:
        assert name in alerted_skills, (
            f"Compromised skill {name!r} must have a CVEAlert"
        )
    for name in clean:
        assert name not in alerted_skills, (
            f"Clean skill {name!r} must NOT have a CVEAlert"
        )


@given(
    skill_name=_skill_name,
    cve_id=_cve_id,
    severity=_severity,
    description=_description,
)
@settings(max_examples=100)
def test_property18_alert_fields_match_api_response(
    skill_name: str,
    cve_id: str,
    severity: str,
    description: str,
) -> None:
    """
    **Property 18 (field fidelity): Campos do CVEAlert correspondem à resposta da API**

    The CVEAlert fields must exactly match the data returned by the ClawHub API —
    no field must be silently dropped, transformed, or defaulted.

    **Validates: Requirements 7.5**
    """
    import asyncio

    cve_record = {"cve_id": cve_id, "severity": severity, "description": description}
    client = FakeClawHubClient({skill_name: [cve_record]})

    result = asyncio.run(check_cve_alerts([skill_name], client))

    assert len(result) == 1
    alert = result[0]
    assert alert.skill_name == skill_name, f"skill_name mismatch: {alert.skill_name!r} != {skill_name!r}"
    assert alert.cve_id == cve_id, f"cve_id mismatch: {alert.cve_id!r} != {cve_id!r}"
    assert alert.severity == severity, f"severity mismatch: {alert.severity!r} != {severity!r}"
    assert alert.description == description, f"description mismatch"


def test_property18_empty_skill_list_returns_no_alerts() -> None:
    """
    **Property 18 (empty list): Lista vazia de skills retorna lista vazia de alertas**

    check_cve_alerts() with an empty installed_skills list must return [].

    **Validates: Requirements 7.5**
    """
    import asyncio

    client = FakeClawHubClient({})
    result = asyncio.run(check_cve_alerts([], client))
    assert result == []
