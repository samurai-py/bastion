"""Invisible snapshot versioning for SKILL.md files (D-07/SKWR-04).

Users never interact with .versions/ directly — rollback via natural language.
"""
from __future__ import annotations

import atexit
import concurrent.futures
import logging
from datetime import UTC, date, datetime, timedelta
from pathlib import Path

logger = logging.getLogger(__name__)
_executor = concurrent.futures.ThreadPoolExecutor(max_workers=1)
# Flush in-flight snapshot writes on interpreter shutdown so a SIGTERM mid-edit
# doesn't silently drop the pre-edit version (IN-02).
atexit.register(_executor.shutdown, wait=True)

VERSIONS_DIR = ".versions"
SNAPSHOT_PREFIX = "SKILL.md."
# Timestamp format — sortable, UTC
_TS_FMT = "%Y%m%dT%H%M%SZ"


def snapshot(skill_path: Path) -> None:
    """Save current SKILL.md to .versions/ before any edit (non-blocking).

    Called by skill_create and skill_edit in mcp_server before writing.
    Content is captured on the CALLER THREAD at submit time — the background
    thread only writes bytes it already received, never re-reads skill_path
    (CR-01 fix: eliminates race with synchronous main-thread overwrite).
    Failure is logged but never propagated — edit must not fail due to versioning.
    """
    if not skill_path.exists():
        return
    ts = datetime.now(UTC).strftime(_TS_FMT)
    dest = skill_path.parent / VERSIONS_DIR / f"{SNAPSHOT_PREFIX}{ts}"
    try:
        content = skill_path.read_text(encoding="utf-8")  # captured NOW, on caller thread
    except Exception as e:
        logger.error("versioning.snapshot: cannot read %s: %s", skill_path, e)
        return

    def _write() -> None:
        try:
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_text(content, encoding="utf-8")
            logger.debug("versioning.snapshot: saved %s", dest.name)
        except Exception as e:
            logger.error("versioning.snapshot failed for %s: %s", skill_path, e)

    _executor.submit(_write)


def list_snapshots(skill_path: Path) -> list[Path]:
    """Return sorted list of snapshot paths (oldest first)."""
    versions_dir = skill_path.parent / VERSIONS_DIR
    if not versions_dir.exists():
        return []
    snaps = sorted(versions_dir.glob(f"{SNAPSHOT_PREFIX}*"))
    return snaps


def _parse_date_hint(date_hint: str) -> date | None:
    """Convert NL date hint to date object.

    Supports: "ontem", "yesterday", "YYYY-MM-DD", "YYYYMMDD".
    """
    h = date_hint.strip().lower()
    if h in ("ontem", "yesterday"):
        return date.today() - timedelta(days=1)
    for fmt in ("%Y-%m-%d", "%Y%m%d"):
        try:
            return datetime.strptime(h, fmt).date()
        except ValueError:
            continue
    return None


def rollback_to_date(skill_path: Path, date_hint: str) -> str | None:
    """Find closest snapshot to date_hint and restore it.

    Returns snapshot filename restored, or None if no snapshot found.
    Used when user says "volta a skill de metas pra ontem" (D-07).
    """
    target_date = _parse_date_hint(date_hint)
    if target_date is None:
        logger.warning("versioning.rollback: unrecognised date_hint '%s'", date_hint)
        return None

    snaps = list_snapshots(skill_path)
    if not snaps:
        logger.warning("versioning.rollback: no snapshots for %s", skill_path)
        return None

    # Find snapshot closest to (and not after) the target date
    target_dt = datetime.combine(target_date, datetime.max.time()).replace(tzinfo=UTC)
    best: Path | None = None
    for snap in reversed(snaps):  # newest first
        try:
            ts_str = snap.name[len(SNAPSHOT_PREFIX):]
            snap_dt = datetime.strptime(ts_str, _TS_FMT).replace(tzinfo=UTC)
            if snap_dt <= target_dt:
                best = snap
                break
        except ValueError:
            continue

    if best is None:
        logger.warning("versioning.rollback: no snapshot <= %s for %s", target_date, skill_path)
        return None

    # Snapshot the CURRENT state before overwriting it, so a rollback is itself
    # reversible (mirrors skill_create/skill_edit, which always snapshot first).
    snapshot(skill_path)
    try:
        skill_path.write_text(best.read_text(encoding="utf-8"), encoding="utf-8")
        logger.info("versioning.rollback: restored %s from %s", skill_path, best.name)
        return best.name
    except Exception as e:
        logger.error("versioning.rollback failed: %s", e)
        return None
