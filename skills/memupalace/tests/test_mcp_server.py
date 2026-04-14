"""Tests for MemupalaceMCPServer — unit tests (9.1) and property test P17 (9.3)."""

from __future__ import annotations

import math

import pytest
from hypothesis import given, settings
from hypothesis import strategies as st

from skills.memupalace.factory import MemupalaceSettings, _create_memupalace_with_embedder
from skills.memupalace.mcp_server import MemupalaceMCPServer


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _make_server(tmp_chroma_path: str, tmp_sqlite_path: str, mock_embedder) -> MemupalaceMCPServer:
    """Build a MemupalaceMCPServer wired with the mock embedder."""
    cfg = MemupalaceSettings(
        chroma_path=tmp_chroma_path,
        sqlite_path=tmp_sqlite_path,
        onnx_model_path="models/embedder.onnx",  # not used — mock embedder
    )
    mp = _create_memupalace_with_embedder(cfg, mock_embedder)
    return MemupalaceMCPServer(memupalace=mp, embedder=mock_embedder)


# ---------------------------------------------------------------------------
# 9.1 Unit tests
# ---------------------------------------------------------------------------


def test_list_tools_registers_5_tools(tmp_chroma_path, tmp_sqlite_path, mock_embedder):
    """All 5 expected tools must be registered."""
    server = _make_server(tmp_chroma_path, tmp_sqlite_path, mock_embedder)
    tools = server.list_tools()
    assert len(tools) == 5
    assert set(tools) == {
        "memory_add",
        "memory_search",
        "memory_list_locations",
        "memory_delete",
        "memory_embed",
    }


def test_memory_add_valid(tmp_chroma_path, tmp_sqlite_path, mock_embedder):
    """memory_add with valid params returns a dict with 'id' and 'operation'."""
    server = _make_server(tmp_chroma_path, tmp_sqlite_path, mock_embedder)
    result = server.call_tool("memory_add", content="Hello world", wing="test")
    assert "id" in result
    assert "operation" in result
    assert result["operation"] in ("created", "reinforced")
    assert isinstance(result["id"], str) and result["id"]


def test_memory_embed_empty_text_raises(tmp_chroma_path, tmp_sqlite_path, mock_embedder):
    """memory_embed with empty string must raise ValueError."""
    server = _make_server(tmp_chroma_path, tmp_sqlite_path, mock_embedder)
    with pytest.raises(ValueError, match="non-empty"):
        server.call_tool("memory_embed", text="")


def test_memory_embed_whitespace_raises(tmp_chroma_path, tmp_sqlite_path, mock_embedder):
    """memory_embed with whitespace-only string must raise ValueError."""
    server = _make_server(tmp_chroma_path, tmp_sqlite_path, mock_embedder)
    with pytest.raises(ValueError, match="non-empty"):
        server.call_tool("memory_embed", text="   ")


def test_memory_delete_nonexistent_raises(tmp_chroma_path, tmp_sqlite_path, mock_embedder):
    """memory_delete with a fake ID must raise KeyError."""
    server = _make_server(tmp_chroma_path, tmp_sqlite_path, mock_embedder)
    with pytest.raises(KeyError):
        server.call_tool("memory_delete", memory_id="00000000-0000-0000-0000-000000000000")


def test_memory_search_returns_list(tmp_chroma_path, tmp_sqlite_path, mock_embedder):
    """After adding a memory, searching for it returns a list."""
    server = _make_server(tmp_chroma_path, tmp_sqlite_path, mock_embedder)
    server.call_tool("memory_add", content="Python is great for data science", wing="tech")
    results = server.call_tool("memory_search", query="Python data science", wing="tech")
    assert isinstance(results, list)
    assert len(results) >= 1
    first = results[0]
    assert "id" in first
    assert "content" in first
    assert "salience_score" in first


def test_call_tool_unknown_raises(tmp_chroma_path, tmp_sqlite_path, mock_embedder):
    """Calling an unknown tool name raises ValueError."""
    server = _make_server(tmp_chroma_path, tmp_sqlite_path, mock_embedder)
    with pytest.raises(ValueError, match="Unknown tool"):
        server.call_tool("nonexistent_tool")


def test_memory_embed_returns_vector(tmp_chroma_path, tmp_sqlite_path, mock_embedder):
    """memory_embed with valid text returns a list of floats."""
    server = _make_server(tmp_chroma_path, tmp_sqlite_path, mock_embedder)
    vec = server.call_tool("memory_embed", text="hello")
    assert isinstance(vec, list)
    assert len(vec) == 384
    assert all(isinstance(v, float) for v in vec)


def test_memory_list_locations_returns_list(tmp_chroma_path, tmp_sqlite_path, mock_embedder):
    """memory_list_locations returns a list of strings."""
    server = _make_server(tmp_chroma_path, tmp_sqlite_path, mock_embedder)
    server.call_tool("memory_add", content="Some fact", wing="personal")
    locs = server.call_tool("memory_list_locations")
    assert isinstance(locs, list)
    assert "personal" in locs


def test_memory_delete_valid(tmp_chroma_path, tmp_sqlite_path, mock_embedder):
    """memory_delete with a valid ID returns confirmation dict."""
    server = _make_server(tmp_chroma_path, tmp_sqlite_path, mock_embedder)
    add_result = server.call_tool("memory_add", content="To be deleted", wing="temp")
    memory_id = add_result["id"]
    result = server.call_tool("memory_delete", memory_id=memory_id)
    assert result == {"deleted": memory_id}


# ---------------------------------------------------------------------------
# 9.3 Property 17: Embedding Service Non-Zero Output
# Validates: Requirements 3.5, 13.1
# ---------------------------------------------------------------------------


def _make_stub_embedder():
    """Build a standalone stub embedder (no pytest fixture) for property tests."""
    import math as _math
    from unittest.mock import MagicMock

    embedder = MagicMock()

    def _embed(text: str) -> list[float]:
        seed = hash(text) % (2**31)
        vec = [_math.sin(seed + i) * 0.1 + 0.01 for i in range(384)]
        norm = _math.sqrt(sum(x * x for x in vec))
        return [x / norm for x in vec]

    embedder.embed.side_effect = _embed
    embedder.embed_batch.side_effect = lambda texts: [_embed(t) for t in texts]
    return embedder


@given(text=st.text(min_size=1).filter(lambda s: s.strip()))
@settings(max_examples=100, deadline=None)
def test_property_17_memory_embed_nonzero_output(text: str) -> None:
    """Property 17: For any non-empty text, memory_embed returns a vector with L2 norm > 0.

    Validates: Requirements 3.5, 13.1
    """
    import os
    import tempfile

    stub = _make_stub_embedder()

    with tempfile.TemporaryDirectory() as tmpdir:
        chroma_dir = os.path.join(tmpdir, "chroma")
        sqlite_path = os.path.join(tmpdir, "knowledge.db")

        cfg = MemupalaceSettings(
            chroma_path=chroma_dir,
            sqlite_path=sqlite_path,
            onnx_model_path="models/embedder.onnx",
        )
        mp = _create_memupalace_with_embedder(cfg, stub)
        server = MemupalaceMCPServer(memupalace=mp, embedder=stub)

        vec = server.call_tool("memory_embed", text=text)

    assert isinstance(vec, list), "Result must be a list"
    assert len(vec) > 0, "Embedding must be non-empty"

    norm = math.sqrt(sum(v * v for v in vec))
    assert norm > 0.0, f"L2 norm must be > 0 for text={text!r}, got norm={norm}"
