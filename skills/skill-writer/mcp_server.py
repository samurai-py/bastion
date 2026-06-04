"""skill-writer MCP server — create/edit/rollback SKILL.md via NL (SKWR-01..06).

Transport: streamable-http, porta 8002 (ou SKILL_WRITER_PORT env var).
Zero raw API keys — generative tasks via CORE_GATEWAY_URL (D-08).
SKWR-05: skill_create e skill_edit consultam memupalace (wing='skill-patterns')
         para enriquecer o prompt de geração com padrões de skills similares.
Security: BASTION_INFER_TOKEN env var is sent as Authorization: Bearer when calling
          /api/infer so the hardened gateway accepts the request.
"""
from __future__ import annotations

import logging
import os
from pathlib import Path

import httpx
from fastmcp import FastMCP

from distiller import enqueue_pending, is_distillation_candidate
from versioning import list_snapshots, rollback_to_date, snapshot

logger = logging.getLogger(__name__)
mcp = FastMCP("skill-writer")

CORE_GATEWAY_URL = os.getenv("CORE_GATEWAY_URL", "http://core:3000/api/infer")
MEMUPALACE_URL = os.getenv("MEMUPALACE_URL", "http://memupalace:8001/mcp")
SKILLS_DIR = Path(os.getenv("SKILLS_DIR", "/skills"))


# ── helpers ──────────────────────────────────────────────────────────────────


def _validate_str(name: str, value: object) -> str:
    if not isinstance(value, str) or not str(value).strip():
        raise ValueError(f"Parameter '{name}' must be a non-empty, non-whitespace string.")
    return str(value)


def _skill_path(skill_name: str, scope: str = "global", persona_slug: str | None = None) -> Path:
    """Resolve skill file path. global → SKILLS_DIR/<name>/SKILL.md.

    Path traversal prevention (T-03-04-01): removes '..' and '/' from name.
    """
    safe_name = skill_name.replace("..", "").replace("/", "-").strip("-")
    if scope == "private" and persona_slug:
        return SKILLS_DIR / "personas" / persona_slug / safe_name / "SKILL.md"
    return SKILLS_DIR / safe_name / "SKILL.md"


def _version_string(path: Path) -> str:
    """Derive simple semver from snapshot count."""
    count = len(list_snapshots(path))
    return f"1.{count}.0"


async def _call_gateway(prompt: str, context_tier: str = "cloud_ok") -> str | None:
    """Call inference gateway on Rust core (D-08).

    Sends Authorization: Bearer <BASTION_INFER_TOKEN> so the hardened /api/infer
    gateway accepts the request. Fallback on any HTTP error: enqueue + return None.
    """
    token = os.getenv("BASTION_INFER_TOKEN", "")
    headers: dict[str, str] = {}
    if token:
        headers["Authorization"] = f"Bearer {token}"

    try:
        async with httpx.AsyncClient() as client:
            resp = await client.post(
                CORE_GATEWAY_URL,
                json={"prompt": prompt, "privacy_tier": context_tier},
                headers=headers,
                timeout=30.0,
            )
            resp.raise_for_status()
            return resp.json().get("text", "")
    except httpx.HTTPError as e:
        logger.error("skill-writer: gateway call failed: %s", e)
        enqueue_pending(prompt, context_tier)
        return None


async def _search_memupalace(query: str, wing: str, limit: int) -> list[dict]:
    """Search memupalace for similar patterns (SKWR-05).

    Wing 'skill-patterns' stores previously generated/distilled SKILL.md summaries.
    Returns empty list on any error — caller treats as optional enrichment.
    """
    try:
        async with httpx.AsyncClient() as client:
            resp = await client.post(
                f"{MEMUPALACE_URL}/call-tool",
                json={"name": "memory_search", "arguments": {
                    "query": query, "wing": wing, "limit": limit,
                }},
                timeout=10.0,
            )
            resp.raise_for_status()
            return resp.json().get("result", []) or []
    except Exception as e:
        logger.warning("skill-writer: memupalace search failed (non-fatal): %s", e)
        return []


async def _build_pattern_context(query: str) -> str:
    """Fetch similar skill patterns from memupalace and format as prompt context (SKWR-05).

    Returns empty string if memupalace is unavailable or has no relevant patterns.
    Each pattern is truncated to 200 chars to prevent prompt injection (T-03-04-05).
    """
    similar = await _search_memupalace(query=query, wing="skill-patterns", limit=3)
    if not similar:
        return ""
    examples = "\n".join(
        f"- {p.get('content', p.get('text', ''))[:200]}"
        for p in similar
        if p.get("content") or p.get("text")
    )
    if not examples:
        return ""
    return f"\n\nSimilar existing skill patterns (use as style reference, do not copy):\n{examples}\n"


# ── tools ─────────────────────────────────────────────────────────────────────


@mcp.tool()
async def skill_create(
    name: str,
    description: str,
    instructions: str,
    scope: str = "global",
    persona_slug: str | None = None,
    context_tier: str = "cloud_ok",
) -> dict:
    """Create a new SKILL.md from NL description (SKWR-02, SKWR-05, D-04).

    SKWR-05: consults memupalace for similar skill patterns before generation,
    injecting them into the prompt so the LLM produces better-aligned output.
    Explicit user request auto-activates (D-04).
    Returns skill_reloaded signal so core SkillsLoader rescans (D-06).
    """
    _validate_str("name", name)
    _validate_str("description", description)
    _validate_str("instructions", instructions)

    path = _skill_path(name, scope, persona_slug)

    # SKWR-05: retrieve similar skill patterns from memupalace BEFORE calling gateway
    pattern_context = await _build_pattern_context(f"{name} {description}")

    # Build generation prompt — inject memupalace patterns if available
    prompt = (
        f"Generate a SKILL.md file for the skill named '{name}'.\n"
        f"Description: {description}\n"
        f"Instructions/behavior: {instructions}\n"
        f"{pattern_context}"
        "Format:\n<name>{name}</name>\n<description>{description}</description>\n"
        "<instructions>{step-by-step instructions}</instructions>\n"
        "Only output the SKILL.md content, nothing else."
    )
    skill_md = await _call_gateway(prompt, context_tier)
    if skill_md is None:
        # Gateway unavailable — return queued status
        return {"skill_reloaded": False, "status": "queued", "skill_name": name}

    # Snapshot existing (if any) then write
    if path.exists():
        snapshot(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(skill_md, encoding="utf-8")

    version = _version_string(path)
    logger.info(
        "skill_create: created %s (%s), pattern_context=%s",
        name, scope, bool(pattern_context),
    )

    return {
        "skill_reloaded": True,
        "skill_name": name,
        "skill_path": str(path),
        "version": version,
        "scope": scope,
    }


@mcp.tool()
async def skill_edit(
    name: str,
    edit_instructions: str,
    scope: str = "global",
    persona_slug: str | None = None,
    context_tier: str = "cloud_ok",
) -> dict:
    """Edit an existing skill via NL instructions (SKWR-03, SKWR-05, D-04).

    SKWR-05: consults memupalace for similar skill patterns before editing
    so edits remain aligned with established skill conventions.
    Always snapshots before editing. Returns skill_reloaded signal (D-06).
    """
    _validate_str("name", name)
    _validate_str("edit_instructions", edit_instructions)

    path = _skill_path(name, scope, persona_slug)
    if not path.exists():
        raise ValueError(f"Skill '{name}' not found at {path}")

    # Mandatory snapshot before any edit
    snapshot(path)
    existing_content = path.read_text(encoding="utf-8")

    # SKWR-05: retrieve similar skill patterns from memupalace BEFORE calling gateway
    pattern_context = await _build_pattern_context(f"{name} {edit_instructions}")

    prompt = (
        f"You have this SKILL.md:\n\n{existing_content}\n\n"
        f"Apply this edit: {edit_instructions}\n"
        f"{pattern_context}"
        "Return the complete updated SKILL.md. Only output the SKILL.md content."
    )
    new_content = await _call_gateway(prompt, context_tier)
    if new_content is None:
        return {"skill_reloaded": False, "status": "queued", "skill_name": name}

    path.write_text(new_content, encoding="utf-8")
    version = _version_string(path)
    logger.info("skill_edit: updated %s (%s)", name, scope)

    return {
        "skill_reloaded": True,
        "skill_name": name,
        "skill_path": str(path),
        "version": version,
        "scope": scope,
    }


@mcp.tool()
def skill_rollback(
    name: str,
    date_hint: str,
    scope: str = "global",
    persona_slug: str | None = None,
) -> dict:
    """Rollback a skill to a previous version by natural-language date hint (D-07/SKWR-04).

    Example date_hint values: "ontem", "yesterday", "2026-06-01", "20260601".
    Returns skill_reloaded signal so core SkillsLoader rescans the restored version (D-06).
    """
    _validate_str("name", name)
    _validate_str("date_hint", date_hint)

    path = _skill_path(name, scope, persona_slug)
    restored = rollback_to_date(path, date_hint)

    if restored is None:
        return {
            "rolled_back": False,
            "reason": f"No snapshot found for date_hint='{date_hint}'",
            "skill_name": name,
        }

    return {
        "skill_reloaded": True,
        "skill_name": name,
        "skill_path": str(path),
        "rolled_back": True,
        "snapshot_used": restored,
        "scope": scope,
    }


@mcp.tool()
def skill_distill_candidate(
    tool_calls: list[str],
    context_tier: str = "cloud_ok",
) -> dict:
    """Evaluate if a completed task is a distillation candidate (D-05/SKWR-06).

    NEVER auto-applies — if candidate, enqueues in pending_distillations.jsonl
    for human approval (D-04/D-11 invariant). Returns approval_required=True always
    when status='queued' to document the invariant explicitly.
    """
    # is_distillation_candidate is zero-LLM (sync); use empty list for memupalace
    # (HTTP search not available sync — heuristic gate is sufficient here)
    def _no_search(q: str, w: str, lim: int) -> list[dict]:
        return []

    candidate, reason = is_distillation_candidate(tool_calls, _no_search)
    if not candidate:
        return {"status": "not_candidate", "reason": reason}

    # Enqueue distillation prompt for approval — D-04/D-11 invariant: never auto-apply
    prompt = (
        f"Distil the following sequence of tool calls into a reusable SKILL.md:\n"
        f"Steps: {', '.join(tool_calls)}\n"
        "Write a concise SKILL.md that captures this reusable method."
    )
    enqueue_pending(prompt, context_tier)
    logger.info("skill_distill_candidate: enqueued candidate (%d steps)", len(tool_calls))

    return {
        "status": "queued",
        "reason": reason,
        "steps_count": len(tool_calls),
        "approval_required": True,  # D-04/D-11 invariant — never auto-applied
    }


@mcp.tool()
def skill_list(scope: str = "global", persona_slug: str | None = None) -> list[dict]:
    """List all SKILL.md files in the skills volume."""
    if scope == "private" and persona_slug:
        base = SKILLS_DIR / "personas" / persona_slug
    else:
        base = SKILLS_DIR
    if not base.exists():
        return []
    skills = []
    for skill_md in sorted(base.rglob("SKILL.md")):
        # Skip .versions/ snapshots
        if ".versions" in skill_md.parts:
            continue
        skills.append({"path": str(skill_md), "name": skill_md.parent.name})
    return skills


if __name__ == "__main__":
    port = int(os.getenv("SKILL_WRITER_PORT", "8002"))
    mcp.run(transport="streamable-http", host="0.0.0.0", port=port)
