---
phase: 06-ecosystem-mobile-opcional-p-s-v1-0
plan: "01"
subsystem: mesh
tags: [mesh, transport, sse, auth, pairing, privacy, egress]
completed: "2026-06-17"
duration_minutes: 35

dependency_graph:
  requires: []
  provides:
    - MeshTransport trait (src/mesh/mod.rs)
    - filter_for_mesh (src/mesh/allowlist.rs)
    - GET /events SSE endpoint
    - POST /mesh/ingest (501 stub)
    - POST /auth/exchange (OTC→JWT)
    - POST /mesh/pair (peer registration)
    - MeshPeerConfig + load_mesh_peers (src/config.rs)
  affects:
    - src/channel/webhook.rs (extended with 4 new routes)
    - src/config.rs (BastionConfig extended with mesh field)
    - src/memory/mod.rs (Belief gained tier field)
    - src/main.rs (daemon_loop updated, serve() call updated)

tech_stack:
  added:
    - age = "0.11" (E2E encryption for mesh envelopes)
    - jsonwebtoken = "9" (HS256 JWT for /auth/exchange)
    - tokio-stream = "0.1" (BroadcastStream for SSE)
  patterns:
    - MeshTransport pluggable trait (async_trait, Send+Sync)
    - Two-stage allowlist filter (tag + egress gate)
    - resolve_owner_or_401 shared auth extractor (CR-03)
    - BroadcastStream SSE with keepalive

key_files:
  created:
    - src/mesh/mod.rs
    - src/mesh/allowlist.rs
    - src/mesh/context_provider.rs
  modified:
    - src/channel/webhook.rs
    - src/config.rs
    - src/main.rs
    - src/memory/mod.rs
    - src/memory/sqlite.rs
    - src/lib.rs
    - Cargo.toml

decisions:
  - "D-02 LOCKED: ONE MeshTransport trait serves mesh, mobile, and cloud relay"
  - "D-03: filter_for_mesh calls check_egress sequentially — reuses WR-04, no new privacy primitive"
  - "D-05: no SafeGuard in OSS — privacy mediation is egress gate + tag allowlist only"
  - "D-07: daemon exposes /events SSE + /auth/exchange + /mesh/pair for Flutter app and mesh peers"
  - "Rule 2 deviation: added tier: Option<PrivacyTier> to Belief struct (required for privacy model)"
---

# Phase 06 Plan 01: Mesh Connectivity Layer — Wave 1 Foundation Summary

ONE pluggable MeshTransport trait + filter_for_mesh privacy filter + four HTTP routes (GET /events SSE, POST /mesh/ingest 501 stub, POST /auth/exchange OTC→JWT, POST /mesh/pair peer registration) establishing the Wave 1 connectivity foundation all other Phase 6 domains depend on.

## Tasks Completed

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | MeshTransport trait + filter_for_mesh + wire types | 92ad2c3 | src/mesh/{mod,allowlist,context_provider}.rs, src/memory/{mod,sqlite}.rs, src/lib.rs, Cargo.toml |
| 2 | SSE + ingest + auth/exchange + pair routes + config loader | 175c74c | src/channel/webhook.rs, src/config.rs, src/main.rs, src/mesh/allowlist.rs |

## Verification Results

- `grep "trait MeshTransport" src/mesh/mod.rs` — FOUND
- `grep "pub fn filter_for_mesh" src/mesh/allowlist.rs` — FOUND
- `cargo test mesh:: --lib` — 5/5 PASS (cloudok_passes, localonly_filtered, tag_not_in_allowlist, no_tag, none_tier)
- `grep "sse_handler\|ingest_handler\|resolve_owner_or_401" src/channel/webhook.rs` — FOUND
- All four routes registered: /events, /mesh/ingest, /auth/exchange, /mesh/pair — FOUND
- `resolve_owner_or_401` call count in webhook.rs — 5 (handle + sse + ingest + pair; /auth/exchange is auth entry point, exempt per plan)
- `grep "NOT_IMPLEMENTED" src/channel/webhook.rs` — FOUND (501 stub confirmed)
- `grep "StatusCode::ACCEPTED" src/channel/webhook.rs` — NOT FOUND (202 never used — T-06-01-01 mitigated)
- `grep "MeshPeerConfig\|load_mesh_peers" src/config.rs` — FOUND
- `grep "age" Cargo.toml` — `age = "0.11"` FOUND
- `cargo build` — 0 errors; 1 pre-existing warning (unrelated anthropic.rs field)
- `cargo test --lib` — 142/142 PASS (was 131 before this plan; +11 new tests)

## Deviations from Plan

### Auto-added Missing Functionality

**1. [Rule 2 - Missing Field] Added `tier: Option<PrivacyTier>` to `Belief` struct**
- **Found during:** Task 1 — `filter_for_mesh` references `b.tier` but actual `Belief` struct had no `tier` field
- **Issue:** The existing `Belief` in `src/memory/mod.rs` only had `id, owner_id, persona_tag, content, weight, is_core`. The plan's privacy model requires tier-awareness on beliefs to block LocalOnly items at the mesh boundary.
- **Fix:** Added `pub tier: Option<PrivacyTier>` to `Belief`; added `Serialize/Deserialize` derives to both `Belief` and `PrivacyTier` (required for `SelectiveSlice` wire serialization); updated both sqlite.rs query_map constructors to set `tier: None` (DB column does not exist yet — defaults to deny-on-ambiguity, consistent with egress gate behavior)
- **Files modified:** `src/memory/mod.rs`, `src/memory/sqlite.rs`
- **Commit:** 92ad2c3

**2. [Rule 3 - Blocking] Fixed `daemon_loop` signature to receive `BastionConfig`**
- **Found during:** Task 2 — `load_mesh_peers(&cfg)` inside `daemon_loop` caused `E0423: expected value, found macro cfg` because `cfg` was not in scope inside the function
- **Fix:** Added `cfg: &bastion::config::BastionConfig` parameter to `daemon_loop`; updated call site in `main()`
- **Files modified:** `src/main.rs`
- **Commit:** 175c74c

## Threat Mitigations Implemented

| Threat ID | Status |
|-----------|--------|
| T-06-01-01 | MITIGATED — /mesh/ingest returns 501; no envelope accepted; no from_owner trusted |
| T-06-01-02 | MITIGATED — resolve_owner_or_401 on sse_handler; 401 before BroadcastStream subscription |
| T-06-01-03 | MITIGATED — filter_for_mesh two-stage filter; 5 unit tests assert LocalOnly invariant |
| T-06-01-04 | MITIGATED — MeshEnvelope.ciphertext typed Vec<u8>; opaque by construction |
| T-06-01-07 | MITIGATED — OTC 5-min TTL, single-use (consumed on exchange) |
| T-06-01-08 | MITIGATED — Pairing token 5-min TTL, single-use, CR-03 required on /mesh/pair |

## Known Stubs

| Stub | File | Reason |
|------|------|--------|
| `ingest_handler` returns 501 | src/channel/webhook.rs | Intentional — Plan 02 wires transport.receive() + age decryption + from_owner verification |
| `MeshSliceProvider::context_for_turn` returns `vec![]` | src/mesh/context_provider.rs | Intentional — Plan 02 implements full TurnContextProvider for remote owner slice injection |
| `Belief.tier` always `None` from DB | src/memory/sqlite.rs | DB schema has no privacy_tier column yet; deny-on-ambiguity is safe default; future plan adds column |

## Self-Check: PASSED

- `src/mesh/mod.rs` — EXISTS
- `src/mesh/allowlist.rs` — EXISTS
- `src/mesh/context_provider.rs` — EXISTS
- `src/channel/webhook.rs` — MODIFIED (sse_handler, ingest_handler, auth_exchange_handler, mesh_pair_handler confirmed)
- `src/config.rs` — MODIFIED (MeshPeerConfig, load_mesh_peers, append_mesh_peer confirmed)
- Commits `92ad2c3` and `175c74c` — VERIFIED in git log
- 142 tests passing — VERIFIED
