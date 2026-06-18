---
phase: 06-ecosystem-mobile-opcional-p-s-v1-0
plan: "06"
subsystem: memory/mesh
tags: [privacy, mesh, beliefs, schema-migration, cr-03, cr-04]
dependency_graph:
  requires: [06-01, 06-02, 06-05]
  provides: [privacy_tier-column, store_belief-tier-param, mesh-egress-unblocked]
  affects: [src/memory, src/session, src/mesh, src/agent, src/proactive, src/hooks]
tech_stack:
  added: []
  patterns: [additive-sqlite-migration, kebab-case-enum-serialization, deny-on-ambiguity]
key_files:
  created: []
  modified:
    - src/session/sqlite.rs
    - src/memory/mod.rs
    - src/memory/sqlite.rs
    - src/mesh/context_provider.rs
    - src/proactive/mod.rs
    - src/agent/dream.rs
    - src/agent/identity.rs
    - src/agent/loop_.rs
    - src/agent/command.rs
    - src/hooks/output_validator.rs
decisions:
  - "CR-03 closed: privacy_tier TEXT column added via idempotent ALTER TABLE; NULL = deny-on-ambiguity (safe default for existing rows)"
  - "CR-04 closed: write_cabinet_synthesis now passes Some(CloudOk) explicitly — synthesis crosses filter_for_mesh; no implicit tier promotion"
  - "Tier serialization: kebab-case strings (cloud-ok / local-only) matching serde rename_all = kebab-case on PrivacyTier enum"
  - "All non-synthesis store_belief callers get None — preserves prior behavior; only write_cabinet_synthesis sets CloudOk"
metrics:
  duration: "~10 min"
  completed: "2026-06-18"
  tasks_completed: 2
  tasks_total: 2
  files_modified: 10
---

# Phase 06 Plan 06: Privacy Tier Column + Mesh Egress Unblock Summary

One-liner: Added `privacy_tier TEXT` column to beliefs table and wired `Option<PrivacyTier>` through `store_belief`/`retrieve_tagged`/`load_core`, fixing the deny-on-ambiguity bug that caused `filter_for_mesh` to strip 100% of beliefs on every sync tick.

## What Was Built

**CR-03 fix — schema + memory read/write path:**
- `src/session/sqlite.rs`: idempotent `ALTER TABLE beliefs ADD COLUMN privacy_tier TEXT` migration after `CREATE TABLE` (safe on fresh and existing DBs via `let _ =`)
- `src/memory/mod.rs`: `store_belief` trait signature gains `tier: Option<PrivacyTier>` as final parameter
- `src/memory/sqlite.rs`: INSERT includes `privacy_tier` column; `retrieve_tagged` and `load_core` SELECT it and map kebab-case strings back to `Option<PrivacyTier>`

**CR-04 fix — synthesis write path:**
- `src/mesh/context_provider.rs`: `write_cabinet_synthesis` now calls `store_belief(..., Some(PrivacyTier::CloudOk))` — Cabinet synthesis beliefs are no longer tier:None and are no longer stripped by `filter_for_mesh`

**Caller updates (all appended `None` to preserve existing behavior):**
- Production: `src/proactive/mod.rs`, `src/agent/dream.rs`, `src/agent/identity.rs`
- Test-only: `src/agent/loop_.rs`, `src/agent/command.rs`, `src/hooks/output_validator.rs`

**Integration test:**
- `test_tier_persists_and_survives_filter_for_mesh` in `src/memory/sqlite.rs` exercises real DB path: stores CloudOk + LocalOnly beliefs, retrieves from SQLite, passes through `filter_for_mesh` — asserts only CloudOk survives

## Verification Results

- `cargo build -p bastion`: exits 0 (0 errors, 1 pre-existing dead_code warning)
- `cargo test -p bastion --lib`: 150 passed, 0 failed
- `grep -c "privacy_tier" src/session/sqlite.rs`: 1
- `grep -c "privacy_tier" src/memory/sqlite.rs`: 4
- `grep -c "tier: Option<PrivacyTier>" src/memory/mod.rs`: 1 (trait) + 1 (impl)
- `grep -c "CloudOk" src/mesh/context_provider.rs`: 2
- `test_tier_persists_and_survives_filter_for_mesh`: passes in 150-test run

## Commits

| Task | Commit | Description |
|------|--------|-------------|
| Task 1 | f16ea59 | feat(06-06): add privacy_tier column + wire through store_belief/retrieve_tagged/load_core (CR-03) |
| Task 2 | dcb7cc1 | feat(06-06): wire CloudOk tier into write_cabinet_synthesis + fix all store_belief callers (CR-04) |

## Deviations from Plan

None — all files were in a partially-updated state matching the plan's intended changes. The implementation was already complete in the working tree; this execution validated and committed it.

## Known Stubs

None.

## Threat Flags

No new security surface introduced. The additive migration (T-06-06-03) gives existing rows `NULL` tier which is the safe deny-on-ambiguity default — accepted per threat register.

## Self-Check: PASSED

- `src/session/sqlite.rs` exists and contains `privacy_tier`
- `src/memory/sqlite.rs` exists and contains `privacy_tier` (4 occurrences)
- `src/memory/mod.rs` exists and contains `tier: Option<PrivacyTier>`
- `src/mesh/context_provider.rs` exists and contains `CloudOk`
- Commits f16ea59 and dcb7cc1 verified in git log
