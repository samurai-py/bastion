"""
Supabase Life Log Adapter — stub implementation of LifeLogProtocol.

Activated when DB_STRATEGY=supabase in .env.
Requires the `supabase-py` package and a Supabase project with the
`interactions` table (same schema as the SQLite adapter).

Expected Supabase table DDL:
    CREATE TABLE interactions (
        id        UUID PRIMARY KEY DEFAULT gen_random_uuid(),
        persona   TEXT NOT NULL,
        intent    TEXT NOT NULL,
        tools     JSONB NOT NULL,
        embedding VECTOR(1536),   -- adjust dimension to match your LLM
        timestamp TIMESTAMPTZ NOT NULL DEFAULT now()
    );

    -- Enable pgvector extension first:
    CREATE EXTENSION IF NOT EXISTS vector;

    -- Cosine similarity index:
    CREATE INDEX ON interactions
        USING ivfflat (embedding vector_cosine_ops)
        WITH (lists = 100);
"""

from __future__ import annotations

import logging
from datetime import datetime, timedelta, timezone  # noqa: F401, TC003
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from .protocols import InteractionRecord, LifeLogProtocol  # noqa: F401

logger = logging.getLogger(__name__)


class SupabaseLifeLogAdapter:
    """
    Concrete LifeLogProtocol adapter backed by Supabase (PostgreSQL + pgvector).

    This is a stub implementation. The full implementation requires the
    `supabase-py` package and a configured Supabase project.

    Raises NotImplementedError for all methods until fully implemented.
    """

    def __init__(self, supabase_url: str, supabase_key: str) -> None:
        self._url = supabase_url
        self._key = supabase_key
        self._client: Any = None
        logger.info("SupabaseLifeLogAdapter initialised (url=%s)", supabase_url)

    def _get_client(self) -> Any:
        """Lazily initialise the Supabase client."""
        if self._client is None:
            try:
                from supabase import create_client  # type: ignore[import-untyped]

                self._client = create_client(self._url, self._key)
                logger.debug("Supabase client created")
            except ImportError as exc:
                raise ImportError(
                    "supabase-py is required for SupabaseLifeLogAdapter. "
                    "Install it with: pip install supabase"
                ) from exc
        return self._client

    # ------------------------------------------------------------------
    # LifeLogProtocol implementation (stub)
    # ------------------------------------------------------------------

    async def log_interaction(
        self,
        persona: str,
        intent: str,
        tools: list[str],
        embedding: list[float],
        timestamp: datetime,
    ) -> str:
        """
        Persist a new interaction to Supabase.

        Stub: raises NotImplementedError until supabase-py integration is complete.
        """
        raise NotImplementedError(
            "SupabaseLifeLogAdapter.log_interaction is not yet implemented. "
            "Set DB_STRATEGY=sqlite to use the local SQLite adapter."
        )

    async def search_similar(
        self,
        query_embedding: list[float],
        persona: str | None,
        limit: int,
        threshold: float,
    ) -> list[InteractionRecord]:
        """
        Find similar interactions using pgvector cosine similarity.

        Stub: raises NotImplementedError until supabase-py integration is complete.
        """
        raise NotImplementedError(
            "SupabaseLifeLogAdapter.search_similar is not yet implemented. "
            "Set DB_STRATEGY=sqlite to use the local SQLite adapter."
        )

    async def get_persona_summary(
        self,
        persona: str,
        days: int,
    ) -> list[InteractionRecord]:
        """
        Return interactions for *persona* within the last *days* days.

        Stub: raises NotImplementedError until supabase-py integration is complete.
        """
        raise NotImplementedError(
            "SupabaseLifeLogAdapter.get_persona_summary is not yet implemented. "
            "Set DB_STRATEGY=sqlite to use the local SQLite adapter."
        )

    async def get_last_interactions(
        self,
        personas: list[str],
    ) -> dict[str, datetime | None]:
        """
        Return the timestamp of the most recent interaction for the given personas.

        Stub: raises NotImplementedError until supabase-py integration is complete.
        """
        raise NotImplementedError(
            "SupabaseLifeLogAdapter.get_last_interactions is not yet implemented. "
            "Set DB_STRATEGY=sqlite to use the local SQLite adapter."
        )
