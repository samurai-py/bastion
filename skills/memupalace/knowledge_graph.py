"""Knowledge Graph backed by SQLite for the memupalace skill."""

from __future__ import annotations

import sqlite3
import uuid
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path


@dataclass
class Entity:
    id: str
    name: str
    type: str
    first_seen_at: str  # ISO 8601


@dataclass
class Relation:
    id: str
    source_id: str
    target_id: str
    relation_type: str
    observed_at: str  # ISO 8601
    memory_id: str


_SCHEMA_SQL = """
CREATE TABLE IF NOT EXISTS entities (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    type          TEXT NOT NULL,
    first_seen_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS relations (
    id            TEXT PRIMARY KEY,
    source_id     TEXT NOT NULL REFERENCES entities(id),
    target_id     TEXT NOT NULL REFERENCES entities(id),
    relation_type TEXT NOT NULL,
    observed_at   TEXT NOT NULL,
    memory_id     TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_entities_name    ON entities(name);
CREATE INDEX IF NOT EXISTS idx_relations_source ON relations(source_id);
CREATE INDEX IF NOT EXISTS idx_relations_target ON relations(target_id);
CREATE INDEX IF NOT EXISTS idx_relations_memory ON relations(memory_id);
"""


class KnowledgeGraph:
    """Persistent knowledge graph stored in a dedicated SQLite file.

    The file is created automatically on first use, along with the full schema.
    This database is intentionally separate from ``db/life-log.db`` (Requirement 11.1).
    """

    def __init__(self, sqlite_path: str) -> None:
        path = Path(sqlite_path)
        path.parent.mkdir(parents=True, exist_ok=True)
        self._conn = sqlite3.connect(str(path), check_same_thread=False)
        self._conn.execute("PRAGMA foreign_keys = ON")
        self._conn.executescript(_SCHEMA_SQL)
        self._conn.commit()

    # ------------------------------------------------------------------
    # Entities
    # ------------------------------------------------------------------

    def upsert_entity(self, name: str, entity_type: str) -> str:
        """Return the entity_id for *name*.

        Creates a new entity if one with that name does not yet exist;
        otherwise returns the id of the existing entity (idempotent).
        """
        row = self._conn.execute(
            "SELECT id FROM entities WHERE name = ?", (name,)
        ).fetchone()
        if row is not None:
            return row[0]

        entity_id = str(uuid.uuid4())
        first_seen_at = datetime.now(tz=timezone.utc).isoformat()
        self._conn.execute(
            "INSERT INTO entities (id, name, type, first_seen_at) VALUES (?, ?, ?, ?)",
            (entity_id, name, entity_type, first_seen_at),
        )
        self._conn.commit()
        return entity_id

    def get_entities(self, memory_id: str) -> list[Entity]:
        """Return all entities that appear in relations linked to *memory_id*."""
        rows = self._conn.execute(
            """
            SELECT DISTINCT e.id, e.name, e.type, e.first_seen_at
            FROM entities e
            JOIN relations r ON (r.source_id = e.id OR r.target_id = e.id)
            WHERE r.memory_id = ?
            """,
            (memory_id,),
        ).fetchall()
        return [Entity(id=r[0], name=r[1], type=r[2], first_seen_at=r[3]) for r in rows]

    # ------------------------------------------------------------------
    # Relations
    # ------------------------------------------------------------------

    def add_relation(
        self,
        source_id: str,
        target_id: str,
        relation_type: str,
        memory_id: str,
    ) -> None:
        """Persist a directed relation between two entities."""
        relation_id = str(uuid.uuid4())
        observed_at = datetime.now(tz=timezone.utc).isoformat()
        self._conn.execute(
            """
            INSERT INTO relations (id, source_id, target_id, relation_type, observed_at, memory_id)
            VALUES (?, ?, ?, ?, ?, ?)
            """,
            (relation_id, source_id, target_id, relation_type, observed_at, memory_id),
        )
        self._conn.commit()

    def get_relations(self, entity_id: str) -> list[Relation]:
        """Return all relations where *entity_id* is source or target."""
        rows = self._conn.execute(
            """
            SELECT id, source_id, target_id, relation_type, observed_at, memory_id
            FROM relations
            WHERE source_id = ? OR target_id = ?
            """,
            (entity_id, entity_id),
        ).fetchall()
        return [
            Relation(
                id=r[0],
                source_id=r[1],
                target_id=r[2],
                relation_type=r[3],
                observed_at=r[4],
                memory_id=r[5],
            )
            for r in rows
        ]

    # ------------------------------------------------------------------
    # Lifecycle
    # ------------------------------------------------------------------

    def close(self) -> None:
        self._conn.close()
