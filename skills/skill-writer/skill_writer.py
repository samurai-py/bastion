"""
Skill Writer — utilities for generating and validating SKILL.md files.

Implements:
- SkillScope enum: PRIVATE (persona-scoped) or GLOBAL (bastion-wide)
- SkillMetadata dataclass: frontmatter fields (name, version, description, triggers)
- SkillContent dataclass: full skill content (metadata + instructions + examples + edge_cases)
- generate_skill_md(): renders a SKILL.md string from SkillContent
- get_skill_path(): returns the correct filesystem path based on scope
- validate_skill_md(): checks that a SKILL.md string has all required fields

Path rules (Requirements 6.5):
  - Private skill  → personas/{slug}/SKILL.md
  - Global skill   → skills/{name}/SKILL.md
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path

# ---------------------------------------------------------------------------
# Enums & dataclasses
# ---------------------------------------------------------------------------


class SkillScope(Enum):
    """Scope of a skill: private to a persona or global to the whole Bastion."""

    PRIVATE = "private"
    GLOBAL = "global"


@dataclass
class SkillMetadata:
    """Frontmatter fields required in every SKILL.md."""

    name: str
    version: str
    description: str
    triggers: list[str] = field(default_factory=list)


@dataclass
class SkillContent:
    """Full content of a skill: metadata + body sections."""

    metadata: SkillMetadata
    instructions: str
    examples: str
    edge_cases: str


# ---------------------------------------------------------------------------
# Required frontmatter keys and body section markers
# ---------------------------------------------------------------------------

_REQUIRED_FRONTMATTER_KEYS: tuple[str, ...] = (
    "name",
    "version",
    "description",
    "triggers",
)

_REQUIRED_BODY_SECTIONS: tuple[str, ...] = (
    "## Instruções",
    "## Exemplos",
    "## Edge Cases",
)

# ---------------------------------------------------------------------------
# generate_skill_md
# ---------------------------------------------------------------------------


def generate_skill_md(content: SkillContent) -> str:
    """
    Generate the full SKILL.md string from a SkillContent object.

    The output contains:
    - YAML frontmatter with name, version, description, triggers
    - Body with ## Instruções, ## Exemplos, ## Edge Cases sections

    Args:
        content: The SkillContent to render.

    Returns:
        A string with the complete SKILL.md content.
    """
    meta = content.metadata

    # Build triggers YAML list
    if meta.triggers:
        triggers_yaml = "\n".join(f"  - {t}" for t in meta.triggers)
    else:
        triggers_yaml = "  []"

    # Derive a display name from the skill name (last segment after /)
    display_name = meta.name.split("/")[-1].replace("-", " ").title()

    skill_md = (
        f"---\n"
        f"name: {meta.name}\n"
        f"version: \"{meta.version}\"\n"
        f"description: >\n"
        f"  {meta.description}\n"
        f"triggers:\n"
        f"{triggers_yaml}\n"
        f"---\n"
        f"\n"
        f"# {display_name}\n"
        f"\n"
        f"## Instruções\n"
        f"\n"
        f"{content.instructions.strip()}\n"
        f"\n"
        f"## Exemplos\n"
        f"\n"
        f"{content.examples.strip()}\n"
        f"\n"
        f"## Edge Cases\n"
        f"\n"
        f"{content.edge_cases.strip()}\n"
    )

    return skill_md


# ---------------------------------------------------------------------------
# get_skill_path
# ---------------------------------------------------------------------------


def get_skill_path(
    scope: SkillScope,
    name: str,
    persona_slug: str | None = None,
) -> Path:
    """
    Return the correct filesystem path for a skill based on its scope.

    Path rules (Requirements 6.5):
      - PRIVATE → personas/{slug}/SKILL.md
      - GLOBAL  → skills/{name}/SKILL.md

    Args:
        scope: SkillScope.PRIVATE or SkillScope.GLOBAL.
        name: The skill name in kebab-case (used as directory name for global skills).
        persona_slug: Required when scope is PRIVATE; the persona's slug.

    Returns:
        A Path object pointing to the SKILL.md location.

    Raises:
        ValueError: If scope is PRIVATE and persona_slug is not provided.
    """
    if scope == SkillScope.PRIVATE:
        if not persona_slug:
            raise ValueError(
                "persona_slug is required for PRIVATE scope skills"
            )
        return Path("personas") / persona_slug / "SKILL.md"

    # GLOBAL
    # Use only the last segment of the name as the directory (strip namespace prefix)
    skill_dir = name.split("/")[-1]
    return Path("skills") / skill_dir / "SKILL.md"


# ---------------------------------------------------------------------------
# validate_skill_md
# ---------------------------------------------------------------------------

_FRONTMATTER_RE = re.compile(r"^---\s*\n(.*?)\n---", re.DOTALL)


def validate_skill_md(content: str) -> bool:
    """
    Validate that a SKILL.md string has all required fields.

    Checks:
    1. YAML frontmatter block is present (delimited by ---)
    2. All required frontmatter keys are present: name, version, description, triggers
    3. All required body sections are present: ## Instruções, ## Exemplos, ## Edge Cases

    Args:
        content: The raw SKILL.md string to validate.

    Returns:
        True if all required fields and sections are present, False otherwise.
    """
    # Check frontmatter block exists
    match = _FRONTMATTER_RE.match(content)
    if not match:
        return False

    frontmatter_block = match.group(1)

    # Check all required frontmatter keys are present
    for key in _REQUIRED_FRONTMATTER_KEYS:
        # Match "key:" at the start of a line (handles both inline and block values)
        if not re.search(rf"^{re.escape(key)}\s*:", frontmatter_block, re.MULTILINE):
            return False

    # Check all required body sections are present
    body = content[match.end():]
    for section in _REQUIRED_BODY_SECTIONS:
        if section not in body:
            return False

    return True


# ---------------------------------------------------------------------------
# CLI Interface for OpenClaw Agent
# ---------------------------------------------------------------------------
def main() -> None:
    import argparse
    import json
    import sys
    
    parser = argparse.ArgumentParser(description="CLI wrapper generated by refactoring")
    parser.add_argument("--action", help="Action to perform")
    parser.add_argument("--args-json", default="{}", help="Arguments as JSON string")
    
    args = parser.parse_args()
    print("Execution of stub CLI for", __file__)
    print("Action:", args.action)
    print("Args:", args.args_json)

if __name__ == "__main__":
    main()
