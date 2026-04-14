"""MCP Server for memupalace — exposes memory tools as a callable registry."""

from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from skills.memupalace.embedder import ONNXEmbedder
    from skills.memupalace.factory import Memupalace


class MemupalaceMCPServer:
    """Simple MCP-style server that dispatches named tool calls.

    No external MCP framework required — tools are registered as a plain dict
    of {name: callable}, making the server easy to test and wire up.
    """

    def __init__(self, memupalace: "Memupalace", embedder: "ONNXEmbedder") -> None:
        self._mp = memupalace
        self._embedder = embedder
        self._tools: dict[str, object] = {
            "memory_add": self._memory_add,
            "memory_search": self._memory_search,
            "memory_list_locations": self._memory_list_locations,
            "memory_delete": self._memory_delete,
            "memory_embed": self._memory_embed,
        }

    # ------------------------------------------------------------------
    # Registry API
    # ------------------------------------------------------------------

    def list_tools(self) -> list[str]:
        """Return the names of all registered tools."""
        return list(self._tools.keys())

    def call_tool(self, name: str, **kwargs: object) -> object:
        """Dispatch a tool call by name.

        Raises:
            ValueError: If *name* is not a registered tool.
        """
        if name not in self._tools:
            raise ValueError(
                f"Unknown tool: '{name}'. Available tools: {self.list_tools()}"
            )
        fn = self._tools[name]
        return fn(**kwargs)  # type: ignore[operator]

    # ------------------------------------------------------------------
    # Tool implementations
    # ------------------------------------------------------------------

    def _memory_add(
        self,
        content: str,
        wing: str,
        hall: str | None = None,
        room: str | None = None,
    ) -> dict:
        """Add or reinforce a memory.

        Returns a dict with keys ``id`` and ``operation``.

        Raises:
            ValueError: If *content* is empty/whitespace or location slugs are invalid.
        """
        if not isinstance(content, str) or not content.strip():
            raise ValueError(
                "Parameter 'content' must be a non-empty, non-whitespace string."
            )
        if not isinstance(wing, str) or not wing.strip():
            raise ValueError(
                "Parameter 'wing' must be a non-empty string."
            )

        result = self._mp.add(content=content, wing=wing, hall=hall, room=room)
        return result.model_dump()

    def _memory_search(
        self,
        query: str,
        wing: str | None = None,
        hall: str | None = None,
        room: str | None = None,
        limit: int = 5,
        min_score: float | None = None,
    ) -> list[dict]:
        """Search memories by semantic similarity.

        Returns a list of dicts, each representing a SearchResult.

        Raises:
            ValueError: If *query* is empty/whitespace or *limit* is not positive.
        """
        if not isinstance(query, str) or not query.strip():
            raise ValueError(
                "Parameter 'query' must be a non-empty, non-whitespace string."
            )
        if not isinstance(limit, int) or limit < 1:
            raise ValueError(
                f"Parameter 'limit' must be a positive integer, got {limit!r}."
            )

        results = self._mp.search(
            query=query,
            wing=wing,
            hall=hall,
            room=room,
            limit=limit,
            min_score=min_score,
        )
        return [r.model_dump() for r in results]

    def _memory_list_locations(
        self,
        wing: str | None = None,
        hall: str | None = None,
    ) -> list[str]:
        """List distinct location values.

        - wing=None → all wings
        - wing set, hall=None → halls in that wing
        - wing+hall set → rooms in that wing+hall
        """
        return self._mp.list_locations(wing=wing, hall=hall)

    def _memory_delete(self, memory_id: str) -> dict[str, str]:
        """Delete a memory by ID.

        Returns ``{"deleted": memory_id}`` on success.

        Raises:
            ValueError: If *memory_id* is empty.
            KeyError: If *memory_id* does not exist in the store.
        """
        if not isinstance(memory_id, str) or not memory_id.strip():
            raise ValueError(
                "Parameter 'memory_id' must be a non-empty string."
            )
        self._mp.delete(memory_id)
        return {"deleted": memory_id}

    def _memory_embed(self, text: str) -> list[float]:
        """Return the embedding vector for *text* using the shared ONNXEmbedder.

        Reuses the already-loaded model — no second model is instantiated.

        Raises:
            ValueError: If *text* is empty or whitespace-only.
        """
        if not isinstance(text, str) or not text.strip():
            raise ValueError(
                "Parameter 'text' must be a non-empty, non-whitespace string. "
                "Cannot embed empty or whitespace-only text."
            )
        return self._embedder.embed(text)


# ---------------------------------------------------------------------------
# Factory
# ---------------------------------------------------------------------------


def create_mcp_server(settings: object) -> "MemupalaceMCPServer":
    """Create a fully wired MCP server (requires a real ONNX model on disk).

    Args:
        settings: A ``MemupalaceSettings`` instance.

    Returns:
        A ``MemupalaceMCPServer`` ready to handle tool calls.
    """
    from skills.memupalace.embedder import ONNXEmbedder
    from skills.memupalace.factory import _create_memupalace_with_embedder

    embedder = ONNXEmbedder(settings.onnx_model_path)  # type: ignore[union-attr]
    memupalace = _create_memupalace_with_embedder(settings, embedder)  # type: ignore[arg-type]
    return MemupalaceMCPServer(memupalace=memupalace, embedder=embedder)
