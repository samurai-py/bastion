"""Factory and facade for the memupalace skill."""

from __future__ import annotations

from datetime import datetime, timezone
from typing import TYPE_CHECKING

from skills.memupalace.models import AddResult, MemupalaceSettings, SearchResult
from skills.memupalace.scorer import salience_score

if TYPE_CHECKING:
    from skills.memupalace.embedder import ONNXEmbedder
    from skills.memupalace.knowledge_graph import KnowledgeGraph
    from skills.memupalace.store import MemoryStore


class Memupalace:
    """Facade that aggregates Store, Embedder, KnowledgeGraph and Scorer."""

    def __init__(
        self,
        store: "MemoryStore",
        embedder: "ONNXEmbedder",
        kg: "KnowledgeGraph",
        settings: MemupalaceSettings,
    ) -> None:
        self._store = store
        self._embedder = embedder
        self._kg = kg
        self._settings = settings

    # ------------------------------------------------------------------
    # add
    # ------------------------------------------------------------------

    def add(
        self,
        content: str,
        wing: str,
        hall: str | None = None,
        room: str | None = None,
    ) -> AddResult:
        """Embed content, check for duplicates, then store or reinforce.

        Raises:
            ValueError: If content is empty/whitespace or location slugs are invalid.
        """
        # Validate content
        if not content or not content.strip():
            raise ValueError("Content cannot be empty or whitespace-only")

        # Validate location slugs via the Memory model validator
        import re
        _LOCATION_PATTERN = re.compile(r"^[a-zA-Z0-9_-]+$")
        for field_name, value in [("wing", wing), ("hall", hall), ("room", room)]:
            if value is not None and not _LOCATION_PATTERN.match(value):
                raise ValueError(
                    f"Location value '{value}' contains invalid characters (field: {field_name})"
                )

        # Embed
        embedding = self._embedder.embed(content)

        # Check duplicate
        duplicate = self._store.check_duplicate(
            embedding, wing, self._settings.duplicate_threshold
        )

        if duplicate is not None:
            self._store.reinforce(duplicate.id)
            return AddResult(id=duplicate.id, operation="reinforced")

        # New memory
        new_id = self._store.add(content, embedding, wing, hall, room)

        # KG update — placeholder for MVP, can be extended later
        # self._kg.upsert_entity(content[:50], "concept")

        return AddResult(id=new_id, operation="created")

    # ------------------------------------------------------------------
    # search
    # ------------------------------------------------------------------

    def search(
        self,
        query: str,
        wing: str | None = None,
        hall: str | None = None,
        room: str | None = None,
        limit: int = 5,
        min_score: float | None = None,
    ) -> list[SearchResult]:
        """Embed query, vector search, score, sort, filter, and slice."""
        embedding = self._embedder.embed(query)

        candidates = self._store.vector_search(
            embedding, wing, hall, room, n_results=limit * 10
        )

        now = datetime.now(tz=timezone.utc)
        scored: list[tuple[float, SearchResult]] = []
        for memory, similarity in candidates:
            days_ago = (now - memory.last_reinforced_at).total_seconds() / 86400
            score = salience_score(
                similarity,
                memory.reinforcement_count,
                days_ago,
                self._settings.recency_decay_days,
            )
            scored.append(
                (
                    score,
                    SearchResult(
                        id=memory.id,
                        content=memory.content,
                        wing=memory.wing,
                        hall=memory.hall,
                        room=memory.room,
                        salience_score=score,
                        reinforcement_count=memory.reinforcement_count,
                        last_reinforced_at=memory.last_reinforced_at,
                    ),
                )
            )

        # Sort descending by salience score
        scored.sort(key=lambda t: t[0], reverse=True)

        results = [r for _, r in scored]

        # Filter by min_score
        if min_score is not None:
            results = [r for r in results if r.salience_score >= min_score]

        return results[:limit]

    # ------------------------------------------------------------------
    # list_locations
    # ------------------------------------------------------------------

    def list_locations(
        self, wing: str | None = None, hall: str | None = None
    ) -> list[str]:
        """Delegate to MemoryStore.list_locations."""
        return self._store.list_locations(wing, hall)

    # ------------------------------------------------------------------
    # delete
    # ------------------------------------------------------------------

    def delete(self, memory_id: str) -> None:
        """Delete a memory by ID; raises KeyError if not found."""
        self._store.delete(memory_id)


# ---------------------------------------------------------------------------
# Factory functions
# ---------------------------------------------------------------------------


def create_memupalace(settings: MemupalaceSettings) -> Memupalace:
    """Build a fully wired Memupalace instance using a real ONNX embedder."""
    from skills.memupalace.embedder import ONNXEmbedder
    from skills.memupalace.knowledge_graph import KnowledgeGraph
    from skills.memupalace.store import MemoryStore

    embedder = ONNXEmbedder(settings.onnx_model_path)
    return _create_memupalace_with_embedder(settings, embedder)


def _create_memupalace_with_embedder(
    settings: MemupalaceSettings, embedder: "ONNXEmbedder"
) -> Memupalace:
    """Build a Memupalace instance with a pre-built embedder (for testing)."""
    from skills.memupalace.knowledge_graph import KnowledgeGraph
    from skills.memupalace.store import MemoryStore

    store = MemoryStore(settings.chroma_path)
    kg = KnowledgeGraph(settings.sqlite_path)
    return Memupalace(store=store, embedder=embedder, kg=kg, settings=settings)
