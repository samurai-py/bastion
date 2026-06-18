---
phase: 06-ecosystem-mobile-opcional-p-s-v1-0
plan: "07"
subsystem: mesh-security
tags: [security, mesh, SSRF, TOML-injection, owner-boundary, CR-06, SEC-01, SEC-02, WR-02, WR-06]
dependency_graph:
  requires: ["06-05"]
  provides: ["owner-boundary-enforcement", "SSRF-protection", "TOML-injection-hardening"]
  affects: ["src/mesh/p2p.rs", "src/channel/webhook.rs", "src/config.rs"]
tech_stack:
  added: ["toml_edit=0.22", "url=2", "regex=1"]
  patterns: ["toml_edit programmatic table construction", "DNS-resolve SSRF guard", "regex input validation", "atomic write via .tmp+rename"]
key_files:
  created: []
  modified:
    - src/mesh/p2p.rs
    - src/channel/webhook.rs
    - src/config.rs
    - src/agent/loop_.rs
    - src/mesh/context_provider.rs
    - Cargo.toml
decisions:
  - "CR-06: ingest_handler reads MESH_OWNER_ID/BASTION_OWNER_ID env var for the owner check (env approach — avoids threading local_owner through AppState)"
  - "SEC-02: DNS failure → 400 fail-closed (prevents attacker probing internal hostnames via retry)"
  - "WR-02: atomic write via .tmp + rename; bail on read error (not unwrap_or_default)"
  - "WR-06: add_mesh_slice_provider reads BASTION_OWNER_ID env var, falls back to DEFAULT_OWNER (not session_id)"
metrics:
  duration_minutes: 9
  completed_date: "2026-06-18"
  tasks_completed: 2
  files_changed: 6
  tests_added: 5
  tests_total: 155
---

# Phase 06 Plan 07: Security Hardening — Owner Boundary + SSRF + TOML Injection Summary

JWT auth with refresh rotation using jose library → Owner-boundary enforcement on /mesh/ingest + SSRF validation on peer_url registration + TOML injection elimination via toml_edit.

## Tasks Completed

| # | Name | Commit | Key Files |
|---|------|--------|-----------|
| 1 | CR-06 owner boundary + redirect policy + WR-06 | 23d7c72 | src/mesh/p2p.rs, src/channel/webhook.rs, src/agent/loop_.rs, src/mesh/context_provider.rs |
| 2 | SEC-01 toml_edit + SEC-02 SSRF validation | 0a8a8ad | src/config.rs, src/channel/webhook.rs, Cargo.toml |

## What Was Built

### Task 1 — CR-06: Owner Boundary Enforcement + SEC-02 (redirect) + WR-06

**P2PTransport::receive() (src/mesh/p2p.rs)**
- Added `to_owner` assertion after `from_owner` mismatch check: `envelope.to_owner != self.local_owner` → `bail!` with descriptive error.
- Built reqwest client with `redirect::Policy::none()` — prevents open-redirect SSRF on outbound mesh send path.
- Unit test `test_receive_rejects_wrong_to_owner` verifies cross-owner rejection using real age key pair.

**ingest_handler (src/channel/webhook.rs)**
- After envelope deserialization, reads `MESH_OWNER_ID` (fallback `BASTION_OWNER_ID`) and compares to `envelope.to_owner`.
- Returns 403 before calling `transport.receive()` — avoids wasted decrypt CPU on misrouted envelopes.
- Belt-and-suspenders with P2PTransport::receive() assertion.

**add_mesh_slice_provider (src/agent/loop_.rs) — WR-06**
- Was: `self.session_id.clone()` as `local_owner` proxy (session UUID, changes on restart).
- Now: reads `BASTION_OWNER_ID` env var, falls back to `MESH_OWNER_ID`, then `DEFAULT_OWNER`.
- session_id is NOT a stable owner identifier — this was a correctness bug.

### Task 2 — SEC-01: TOML Injection Hardening + SEC-02: SSRF Validation

**append_mesh_peer rewrite (src/config.rs)**
- Replaced `format!()` string concatenation with `toml_edit` programmatic table construction.
- `validate_age_pubkey()`: regex `^age1[0-9a-z]+$` checked before any file I/O.
- WR-02: `bail!` on read error (was `unwrap_or_default()` — would overwrite entire config).
- WR-02: `toml_edit` parse preserves existing entries including `allowed_tags`.
- Atomic write: write to `bastion.toml.tmp` then rename — prevents partial write corruption.
- New signature: `append_mesh_peer(owner_id, peer_url, age_pubkey, allowed_tags: &[String])`.

**mesh_pair_handler (src/channel/webhook.rs) — SEC-01 + SEC-02**
- `is_private_ip()` helper: blocks loopback (127.x, ::1), RFC1918, link-local (169.254.x), unspecified, broadcast, ULA (fc00::/7).
- age_pubkey regex validation before register — returns 400 on mismatch.
- `url::Url::parse()` + https-scheme check — returns 400 on non-https URL.
- `tokio::net::lookup_host()` DNS resolution + `is_private_ip()` check on all resolved addresses.
- DNS failure → 400 fail-closed (attacker cannot probe internal hostnames via retry).

## Verification Results

```
cargo build -p bastion → Finished (0 errors)
cargo test -p bastion --lib → 155 passed (5 new tests)
grep -c "envelope.to_owner" src/mesh/p2p.rs → 3
grep -c "envelope.to_owner" src/channel/webhook.rs → 2
grep -c "toml_edit" src/config.rs → 4
grep -c "toml_edit" Cargo.toml → 1
grep -c "redirect::Policy::none" src/mesh/p2p.rs → 1
grep -c "validate_age_pubkey" src/config.rs → 6
grep -c "lookup_host\|is_private_ip" src/channel/webhook.rs → 3
grep -c "test_receive_rejects_wrong_to_owner" src/mesh/p2p.rs → 1
grep -c "test_append_mesh_peer_rejects_invalid_age_pubkey" src/config.rs → 1
```

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] age::x25519::Identity::to_string() returns SecretString, not String**
- Found during: Task 1 test compilation
- Issue: Plan test code used `identity.to_string()` expecting `String`; age 0.11 returns `SecretBox<str>`.
- Fix: `use age::secrecy::ExposeSecret as _; identity.to_string().expose_secret().to_owned()`
- Files modified: src/mesh/p2p.rs (test only)
- Commit: 23d7c72

**2. [Rule 2 - Missing critical functionality] DNS failure not addressed in plan**
- Found during: Task 2 implementation of SEC-02 block
- Issue: Plan showed DNS resolve + reject loop but did not specify behavior when `lookup_host` fails.
- Fix: Added `Err(e)` arm → return 400 fail-closed with `"peer_url DNS resolution failed"`.
- Rationale: attacker could probe internal hostnames by crafting names that fail public DNS but resolve internally.
- Files modified: src/channel/webhook.rs
- Commit: 0a8a8ad

None — plan executed with minor runtime API deviation (age SecretString) auto-corrected.

## Decisions Made

1. **CR-06 env approach**: `ingest_handler` reads `MESH_OWNER_ID`/`BASTION_OWNER_ID` from env rather than threading `local_owner` through `AppState`. Belt-and-suspenders with `P2PTransport::receive()` assertion which provides the definitive enforcement.
2. **SEC-02 DNS fail-closed**: DNS resolution failure → 400 (not pass-through). Prevents internal hostname probing.
3. **WR-02 atomic write**: `.tmp` + rename pattern for config write. Simple, no additional deps.
4. **WR-06 env fallback chain**: `BASTION_OWNER_ID` → `MESH_OWNER_ID` → `DEFAULT_OWNER`. Consistent with how main.rs resolves local_owner for P2PTransport.

## Known Stubs

None — all security fixes are wired to live code paths.

## Threat Surface Scan

No new network endpoints, auth paths, or schema changes introduced. All changes are hardening of existing endpoints (/mesh/ingest, /mesh/pair) and config write path. No new threat surface.

## Self-Check: PASSED

- src/mesh/p2p.rs — present, contains `envelope.to_owner`, `redirect::Policy::none`, `test_receive_rejects_wrong_to_owner`
- src/channel/webhook.rs — present, contains `envelope.to_owner`, `is_private_ip`, `lookup_host`
- src/config.rs — present, contains `toml_edit`, `validate_age_pubkey`, `test_append_mesh_peer_rejects_invalid_age_pubkey`
- Cargo.toml — present, contains `toml_edit`
- Commit 23d7c72 — exists (Task 1)
- Commit 0a8a8ad — exists (Task 2)
- 155 lib tests pass
