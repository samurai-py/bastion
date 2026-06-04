"""Tests for distiller.py (D-04/D-05/SKWR-06)."""
from __future__ import annotations

import json
from pathlib import Path

import pytest


def _search_returns_result(query, wing, limit):
    return [{"id": "abc", "content": "file search + parse + validate + write"}]


def _search_returns_empty(query, wing, limit):
    return []


def _search_raises(query, wing, limit):
    raise RuntimeError("memupalace down")


class TestIsDistillationCandidate:
    def test_too_few_steps_returns_false(self):
        from distiller import MIN_STEPS, is_distillation_candidate

        short = ["tool_a", "tool_b"]
        ok, reason = is_distillation_candidate(short, _search_returns_result)
        assert ok is False
        assert str(MIN_STEPS) in reason

    def test_enough_steps_but_no_similar_returns_false(self):
        from distiller import is_distillation_candidate

        calls = ["tool_a", "tool_b", "tool_c", "tool_d"]
        ok, reason = is_distillation_candidate(calls, _search_returns_empty)
        assert ok is False
        assert "similar" in reason.lower()

    def test_candidate_with_enough_steps_and_similar_found(self):
        from distiller import is_distillation_candidate

        calls = ["tool_a", "tool_b", "tool_c", "tool_d", "tool_e"]
        ok, reason = is_distillation_candidate(calls, _search_returns_result)
        assert ok is True
        assert "Recurrent" in reason

    def test_search_exception_returns_false_not_raises(self):
        from distiller import is_distillation_candidate

        calls = ["tool_a", "tool_b", "tool_c", "tool_d"]
        ok, reason = is_distillation_candidate(calls, _search_raises)
        assert ok is False
        assert "failed" in reason.lower()

    def test_exactly_min_steps_with_similar_is_candidate(self):
        from distiller import MIN_STEPS, is_distillation_candidate

        calls = [f"tool_{i}" for i in range(MIN_STEPS)]
        ok, reason = is_distillation_candidate(calls, _search_returns_result)
        assert ok is True

    def test_reason_contains_step_count_when_candidate(self):
        from distiller import is_distillation_candidate

        calls = ["a", "b", "c", "d", "e"]
        ok, reason = is_distillation_candidate(calls, _search_returns_result)
        assert ok is True
        assert "5" in reason


class TestEnqueuePending:
    def test_enqueue_creates_jsonl_entry(self, tmp_path, monkeypatch):
        pending_file = tmp_path / "pending_distillations.jsonl"
        import distiller
        monkeypatch.setattr(distiller, "PENDING_FILE", pending_file)
        from distiller import enqueue_pending

        enqueue_pending("summarise meeting notes", "cloud_ok")
        lines = pending_file.read_text(encoding="utf-8").strip().split("\n")
        assert len(lines) == 1
        entry = json.loads(lines[0])
        assert entry["status"] == "pending"
        assert entry["privacy_tier"] == "cloud_ok"
        assert "summarise" in entry["prompt"]

    def test_enqueue_appends_multiple_entries(self, tmp_path, monkeypatch):
        pending_file = tmp_path / "pending_distillations.jsonl"
        import distiller
        monkeypatch.setattr(distiller, "PENDING_FILE", pending_file)
        from distiller import enqueue_pending

        enqueue_pending("prompt1", "cloud_ok")
        enqueue_pending("prompt2", "local_only")
        lines = [ln for ln in pending_file.read_text(encoding="utf-8").strip().split("\n") if ln]
        assert len(lines) == 2

    def test_enqueue_entry_has_timestamp(self, tmp_path, monkeypatch):
        pending_file = tmp_path / "pending_distillations.jsonl"
        import distiller
        monkeypatch.setattr(distiller, "PENDING_FILE", pending_file)
        from distiller import enqueue_pending

        enqueue_pending("test prompt", "local_only")
        entry = json.loads(pending_file.read_text(encoding="utf-8").strip())
        assert "timestamp" in entry
        assert entry["timestamp"]  # non-empty

    def test_enqueue_creates_parent_dir(self, tmp_path, monkeypatch):
        pending_file = tmp_path / "nested" / "dir" / "pending_distillations.jsonl"
        import distiller
        monkeypatch.setattr(distiller, "PENDING_FILE", pending_file)
        from distiller import enqueue_pending

        enqueue_pending("test", "cloud_ok")
        assert pending_file.exists()
