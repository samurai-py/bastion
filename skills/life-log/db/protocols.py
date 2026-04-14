"""
Life Log Protocols — hexagonal port definitions for the life-log skill.

Defines:
- InteractionRecord: dataclass representing a stored interaction
- LifeLogProtocol: Protocol (port) that all adapters must satisfy
"""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime
from typing import Protocol, runtime_checkable


# ---------------------------------------------------------------------------
# Domain model
# ---------------------------------------------------------------------------


@dataclass
class InteractionRecord:
    """A single interaction entry stored in the life log."""

    id: str
    persona: str
    intent: str
    tools: list[str]
    embedding: list[float]
    timestamp: datetime


# ---------------------------------------------------------------------------
# Life Log Protocol (hexagonal port)
# ---------------------------------------------------------------------------


@runtime_checkable
class LifeLogProtocol(Protocol):
    """Port for logging and querying persona interactions."""

    async def log_interaction(
        self,
        persona: str,
        intent: str,
        tools: list[str],
        embedding: list[float],
        timestamp: datetime,
    ) -> str:
        """
        Persist a new interaction record.

        Args:
            persona: Slug of the active persona.
            intent: Description of the executed intent.
            tools: List of tool names used in this interaction.
            embedding: Vector embedding of the input text.
            timestamp: UTC datetime of the interaction.

        Returns:
            The unique identifier (UUID) of the created record.
        """
        ...

    async def search_similar(
        self,
        query_embedding: list[float],
        persona: str | None,
        limit: int,
        threshold: float,
    ) -> list[InteractionRecord]:
        """
        Find interactions semantically similar to *query_embedding*.

        Args:
            query_embedding: Vector to compare against stored embeddings.
            persona: If provided, restrict results to this persona slug.
            limit: Maximum number of results to return.
            threshold: Minimum cosine similarity score (0.0–1.0).

        Returns:
            List of InteractionRecord ordered by descending similarity.
            Only records with similarity >= threshold are included.
        """
        ...

    async def get_persona_summary(
        self,
        persona: str,
        days: int,
    ) -> list[InteractionRecord]:
        """
        Return all interactions for *persona* within the last *days* days.

        Args:
            persona: Slug of the persona to summarise.
            days: Number of days to look back from now (UTC).

        Returns:
            List of InteractionRecord ordered by descending timestamp.
        """
        ...

    async def get_last_interactions(
        self,
        personas: list[str],
    ) -> dict[str, datetime | None]:
        """
        Return the timestamp of the most recent interaction for the given personas.

        Args:
            personas: List of persona slugs to check.

        Returns:
            A dictionary mapping each persona slug to its most recent interaction
            timestamp, or None if no interactions exist.
        """
        ...
