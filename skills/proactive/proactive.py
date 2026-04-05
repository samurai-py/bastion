"""
Proactive — inactivity alerts and CVE alerts.

Implements:
- PersonaActivity dataclass: {persona_slug, last_interaction, days_inactive}
- CVEAlert dataclass: {skill_name, cve_id, severity, description}
- LifeLogAdapter Protocol: interface for querying the life log
- ClawHubClient Protocol: interface for querying the ClawHub API
- check_inactive_personas(): returns personas inactive for >= threshold_days
- check_cve_alerts(): returns CVEAlerts for any installed skill with known CVEs

Architecture (hexagonal):
  - LifeLogAdapter and ClawHubClient are Protocols — never import concrete adapters here.
  - Callers inject the concrete adapter at runtime (factory / DI).

Requirements: 7.2, 7.5
"""

from __future__ import annotations

import logging
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Protocol, runtime_checkable

from i18n import get_string, load_locale

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Domain models
# ---------------------------------------------------------------------------


@dataclass
class PersonaActivity:
    """Activity summary for a single persona."""

    persona_slug: str
    last_interaction: datetime | None
    days_inactive: int


@dataclass
class CVEAlert:
    """A CVE alert for an installed skill."""

    skill_name: str
    cve_id: str
    severity: str
    description: str


# ---------------------------------------------------------------------------
# Protocols (hexagonal ports)
# ---------------------------------------------------------------------------


@runtime_checkable
class LifeLogAdapter(Protocol):
    """Port for querying the life log — implemented by SQLite or Supabase adapters."""

    async def get_last_interaction(self, persona_slug: str) -> datetime | None:
        """
        Return the timestamp of the most recent interaction for the given persona,
        or None if no interactions have been recorded.
        """
        ...


@runtime_checkable
class ClawHubClient(Protocol):
    """Port for querying the ClawHub API — implemented by the HTTP adapter."""

    async def get_cves(self, skill_name: str) -> list[dict[str, str]]:
        """
        Return a list of CVE records for the given skill name.

        Each record is a dict with at least:
          - "cve_id": str
          - "severity": str  (e.g. "CRITICAL", "HIGH", "MEDIUM", "LOW")
          - "description": str
        """
        ...


# ---------------------------------------------------------------------------
# check_inactive_personas
# ---------------------------------------------------------------------------


async def check_inactive_personas(
    personas: list[str],
    life_log_adapter: LifeLogAdapter,
    threshold_days: int = 3,
) -> list[PersonaActivity]:
    """
    Check which personas have been inactive for >= threshold_days.

    For each persona slug in `personas`, queries the life log for the most
    recent interaction timestamp. Personas with no interaction or whose last
    interaction was >= threshold_days ago are included in the result.

    Args:
        personas: List of persona slugs to check.
        life_log_adapter: Adapter implementing LifeLogAdapter Protocol.
        threshold_days: Minimum days of inactivity to flag (default: 3).

    Returns:
        List of PersonaActivity for personas inactive >= threshold_days.
        Sorted by days_inactive descending (most inactive first).
    """
    now = datetime.now(tz=timezone.utc)
    inactive: list[PersonaActivity] = []

    for slug in personas:
        last = await life_log_adapter.get_last_interaction(slug)

        if last is None:
            days_inactive = threshold_days  # treat as at least threshold
            logger.debug("Persona %r has no interaction history — treating as inactive", slug)
        else:
            # Ensure last is timezone-aware for comparison
            if last.tzinfo is None:
                last = last.replace(tzinfo=timezone.utc)
            delta = now - last
            days_inactive = int(delta.total_seconds() // 86400)
            logger.debug(
                "Persona %r last interaction: %s (%d days ago)",
                slug,
                last.isoformat(),
                days_inactive,
            )

        if days_inactive >= threshold_days:
            inactive.append(
                PersonaActivity(
                    persona_slug=slug,
                    last_interaction=last,
                    days_inactive=days_inactive,
                )
            )
            logger.info(
                "Inactive persona detected: slug=%r days_inactive=%d",
                slug,
                days_inactive,
            )

    inactive.sort(key=lambda p: p.days_inactive, reverse=True)
    return inactive


# ---------------------------------------------------------------------------
# check_cve_alerts
# ---------------------------------------------------------------------------


async def check_cve_alerts(
    installed_skills: list[str],
    clawhub_client: ClawHubClient,
) -> list[CVEAlert]:
    """
    Check all installed skills for known CVEs via the ClawHub API.

    For each skill name in `installed_skills`, queries the ClawHub API for
    CVE records. Returns a CVEAlert for every CVE found across all skills.

    Args:
        installed_skills: List of skill names to check (e.g. ["bastion/life-log", "github"]).
        clawhub_client: Client implementing ClawHubClient Protocol.

    Returns:
        List of CVEAlert for every CVE found. Empty list if no CVEs detected.
        Sorted by skill_name then cve_id for deterministic output.
    """
    alerts: list[CVEAlert] = []

    for skill_name in installed_skills:
        try:
            cves = await clawhub_client.get_cves(skill_name)
        except Exception:
            logger.warning(
                "ClawHub API unavailable for skill %r — skipping CVE check",
                skill_name,
                exc_info=True,
            )
            continue

        for cve in cves:
            alert = CVEAlert(
                skill_name=skill_name,
                cve_id=cve["cve_id"],
                severity=cve["severity"],
                description=cve["description"],
            )
            alerts.append(alert)
            logger.warning(
                "CVE detected: skill=%r cve_id=%r severity=%r",
                skill_name,
                cve["cve_id"],
                cve["severity"],
            )

    alerts.sort(key=lambda a: (a.skill_name, a.cve_id))
    return alerts

# ---------------------------------------------------------------------------
# CLI Interface for OpenClaw Agent
# ---------------------------------------------------------------------------
def main() -> None:
    import argparse
    import asyncio
    import sys
    import json

    parser = argparse.ArgumentParser(description="Proactive Skill CLI")
    parser.add_argument("--language", default="en", help="Locale language tag (e.g. en, pt-BR)")
    subparsers = parser.add_subparsers(dest="command", required=True)

    # inactive
    inactive_parser = subparsers.add_parser("inactive", help="Check inactive personas")
    inactive_parser.add_argument("--personas", help="JSON list of persona slugs", required=True)
    inactive_parser.add_argument("--threshold", type=int, default=3)

    # cve
    cve_parser = subparsers.add_parser("cve", help="Check CVE alerts")
    cve_parser.add_argument("--skills", help="JSON list of installed skills", required=True)

    args = parser.parse_args()

    async def run():
        locale = load_locale(args.language, skill_dir=Path(__file__).parent)

        if args.command == "inactive":
            try:
                import sys
                import os
                sys.path.append(os.path.join(os.path.dirname(__file__), "..", ".."))
                from skills.life_log.factory import Settings, create_adapter
                settings = Settings.from_env()
                adapter = create_adapter(settings)
            except Exception as e:
                print(get_string(locale, "warning_adapter", error=e))
                # Mock adapter for fallback
                class MockAdapter:
                    async def get_last_interaction(self, slug):
                        return None
                adapter = MockAdapter()
            
            personas_list = json.loads(args.personas)
            results = await check_inactive_personas(personas_list, adapter, args.threshold)
            if not results:
                print(get_string(locale, "all_active"))
            for r in results:
                print(get_string(locale, "persona_inactive", slug=r.persona_slug, days=r.days_inactive))

        elif args.command == "cve":
            class DummyClawHubClient:
                async def get_cves(self, skill_name: str):
                    return []

            skills_list = json.loads(args.skills)
            results = await check_cve_alerts(skills_list, DummyClawHubClient())
            if not results:
                print(get_string(locale, "no_cves"))
            for alert in results:
                print(get_string(locale, "cve_alert",
                                 severity=alert.severity, skill=alert.skill_name,
                                 cve_id=alert.cve_id, description=alert.description))

    asyncio.run(run())

if __name__ == "__main__":
    main()
