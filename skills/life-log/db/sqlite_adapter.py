"""
SQLite Life Log Adapter — concrete implementation using SQLite + sqlite-vec.

Creates db/life-log.db automatically on first write.
Uses sqlite-vec extension for cosine similarity vector search.

Schema:
    interactions (
        id        TEXT PRIMARY KEY,
        persona   TEXT NOT NULL,
        intent    TEXT NOT NULL,
        tools     TEXT NOT NULL,   -- JSON array
        embedding BLOB NOT NULL,   -- sqlite-vec float32 vector
        timestamp TEXT NOT NULL    -- ISO 8601 UTC
    )
"""

from __future__ import annotations

import json
import logging
import sqlite3
import struct
import uuid
from datetime import datetime, timezone
from pathlib import Path

from .protocols import InteractionRecord, LifeLogProtocol

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _encode_embedding(embedding: list[float]) -> bytes:
    """Encode a list of floats as a little-endian float32 BLOB."""
    return struct.pack(f"{len(embedding)}f", *embedding)


def _decode_embedding(blob: bytes) -> list[float]:
    """Decode a little-endian float32 BLOB back to a list of floats."""
    count = len(blob) // 4
    return list(struct.unpack(f"{count}f", blob))


def _cosine_similarity(a: list[float], b: list[float]) -> float:
    """Compute cosine similarity between two equal-length vectors."""
    if len(a) != len(b):
        raise ValueError("Vectors must have the same dimension")
    dot = sum(x * y for x, y in zip(a, b))
    norm_a = sum(x * x for x in a) ** 0.5
    norm_b = sum(x * x for x in b) ** 0.5
    if norm_a == 0.0 or norm_b == 0.0:
        return 0.0
    return dot / (norm_a * norm_b)


# ---------------------------------------------------------------------------
# DDL
# ---------------------------------------------------------------------------

_CREATE_TABLE = """
CREATE TABLE IF NOT EXISTS interactions (
    id        TEXT PRIMARY KEY,
    persona   TEXT NOT NULL,
    intent    TEXT NOT NULL,
    tools     TEXT NOT NULL,
    embedding BLOB NOT NULL,
    timestamp TEXT NOT NULL
);
"""

_CREATE_INDEX_PERSONA = """
CREATE INDEX IF NOT EXISTS idx_interactions_persona
    ON interactions (persona);
"""

_CREATE_INDEX_TIMESTAMP = """
CREATE INDEX IF NOT EXISTS idx_interactions_timestamp
    ON interactions (timestamp);
"""


# ---------------------------------------------------------------------------
# Adapter
# ---------------------------------------------------------------------------


class SQLiteLifeLogAdapter:
    """
    Concrete LifeLogProtocol adapter backed by SQLite.

    sqlite-vec is loaded as an extension when available; if the extension
    is not installed the adapter falls back to pure-Python cosine similarity
    so the skill works offline without any native dependencies.

    The database file is created automatically on the first write operation.
    """

    def __init__(self, db_path: str | Path = "db/life-log.db") -> None:
        self._db_path = Path(db_path)
        self._initialised = False

    # ------------------------------------------------------------------
    # Internal helpers
    # ------------------------------------------------------------------

    def _connect(self) -> sqlite3.Connection:
        """Open (and optionally create) the SQLite database."""
        self._db_path.parent.mkdir(parents=True, exist_ok=True)
        conn = sqlite3.connect(str(self._db_path))
        conn.row_factory = sqlite3.Row

        # Attempt to load sqlite-vec extension (optional)
        try:
            conn.enable_load_extension(True)
            import sqlite_vec  # type: ignore[import-untyped]
            sqlite_vec.load(conn)
            logger.debug("sqlite-vec extension loaded")
        except Exception as exc:  # noqa: BLE001
            logger.debug("sqlite-vec not available, using fallback: %s", exc)

        return conn

    def _ensure_schema(self, conn: sqlite3.Connection) -> None:
        """Create tables and indexes if they do not exist."""
        conn.execute(_CREATE_TABLE)
        conn.execute(_CREATE_INDEX_PERSONA)
        conn.execute(_CREATE_INDEX_TIMESTAMP)
        conn.commit()

    def _get_connection(self) -> sqlite3.Connection:
        """Return a connection with schema guaranteed to exist."""
        conn = self._connect()
        self._ensure_schema(conn)
        return conn

    @staticmethod
    def _row_to_record(row: sqlite3.Row) -> InteractionRecord:
        """Convert a database row to an InteractionRecord."""
        return InteractionRecord(
            id=row["id"],
            persona=row["persona"],
            intent=row["intent"],
            tools=json.loads(row["tools"]),
            embedding=_decode_embedding(row["embedding"]),
            timestamp=datetime.fromisoformat(row["timestamp"]),
        )

    # ------------------------------------------------------------------
    # LifeLogProtocol implementation
    # ------------------------------------------------------------------

    async def log_interaction(
        self,
        persona: str,
        intent: str,
        tools: list[str],
        embedding: list[float],
        timestamp: datetime,
    ) -> str:
        """Persist a new interaction and return its UUID."""
        interaction_id = str(uuid.uuid4())
        tools_json = json.dumps(tools)
        embedding_blob = _encode_embedding(embedding)
        # Always store as UTC ISO 8601
        ts_str = timestamp.astimezone(timezone.utc).isoformat()

        conn = self._get_connection()
        try:
            conn.execute(
                """
                INSERT INTO interactions (id, persona, intent, tools, embedding, timestamp)
                VALUES (?, ?, ?, ?, ?, ?)
                """,
                (interaction_id, persona, intent, tools_json, embedding_blob, ts_str),
            )
            conn.commit()
        finally:
            conn.close()

        logger.info(
            "Interaction logged: id=%s persona=%s intent=%r",
            interaction_id,
            persona,
            intent,
        )
        return interaction_id

    async def search_similar(
        self,
        query_embedding: list[float],
        persona: str | None,
        limit: int,
        threshold: float,
    ) -> list[InteractionRecord]:
        """
        Return up to *limit* interactions with cosine similarity >= *threshold*.

        Falls back to pure-Python similarity when sqlite-vec is unavailable.
        """
        conn = self._get_connection()
        try:
            if persona is not None:
                rows = conn.execute(
                    "SELECT * FROM interactions WHERE persona = ?",
                    (persona,),
                ).fetchall()
            else:
                rows = conn.execute("SELECT * FROM interactions").fetchall()
        finally:
            conn.close()

        # Compute cosine similarity in Python (works with or without sqlite-vec)
        scored: list[tuple[float, InteractionRecord]] = []
        for row in rows:
            record = self._row_to_record(row)
            sim = _cosine_similarity(query_embedding, record.embedding)
            if sim >= threshold:
                scored.append((sim, record))

        # Sort by descending similarity, apply limit
        scored.sort(key=lambda t: t[0], reverse=True)
        results = [record for _, record in scored[:limit]]

        logger.debug(
            "search_similar: %d candidates, %d above threshold=%.2f",
            len(rows),
            len(results),
            threshold,
        )
        return results

    async def get_persona_summary(
        self,
        persona: str,
        days: int,
    ) -> list[InteractionRecord]:
        """Return all interactions for *persona* within the last *days* days."""
        from datetime import timedelta

        cutoff = datetime.now(tz=timezone.utc) - timedelta(days=days)
        cutoff_str = cutoff.isoformat()

        conn = self._get_connection()
        try:
            rows = conn.execute(
                """
                SELECT * FROM interactions
                WHERE persona = ?
                  AND timestamp >= ?
                ORDER BY timestamp DESC
                """,
                (persona, cutoff_str),
            ).fetchall()
        finally:
            conn.close()

        records = [self._row_to_record(row) for row in rows]
        logger.debug(
            "get_persona_summary: persona=%s days=%d → %d records",
            persona,
            days,
            len(records),
        )
        return records

    async def get_last_interactions(
        self,
        personas: list[str],
    ) -> dict[str, datetime | None]:
        """Return the timestamp of the most recent interaction for the given personas."""
        if not personas:
            return {}

        conn = self._get_connection()
        try:
            placeholders = ",".join("?" for _ in personas)
            # Use GROUP BY to efficiently get the MAX timestamp for each persona
            rows = conn.execute(
                f"""
                SELECT persona, MAX(timestamp) as last_ts
                FROM interactions
                WHERE persona IN ({placeholders})
                GROUP BY persona
                """,
                tuple(personas),
            ).fetchall()
        finally:
            conn.close()

        # Initialize all requested personas with None
        result: dict[str, datetime | None] = {p: None for p in personas}

        # Update with actual timestamps found
        for row in rows:
            persona = row["persona"]
            ts_str = row["last_ts"]
            if ts_str:
                result[persona] = datetime.fromisoformat(ts_str)

        logger.debug(
            "get_last_interactions: checked %d personas, found %d active",
            len(personas),
            sum(1 for v in result.values() if v is not None),
        )
        return result
