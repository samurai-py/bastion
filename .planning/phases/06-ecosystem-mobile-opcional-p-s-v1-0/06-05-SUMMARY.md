---
phase: 06-ecosystem-mobile-opcional-p-s-v1-0
plan: "05"
subsystem: channel/webhook + mobile/flutter
tags: [jwt, auth, mobile, security, tdd]
dependency_graph:
  requires: [06-01, 06-03]
  provides: [working-mobile-auth-chain]
  affects: [src/channel/webhook.rs, mobile/lib/services/api_service.dart, src/main.rs]
tech_stack:
  added: []
  patterns: [jsonwebtoken::decode HS256, OtcStore type alias, fail-closed env var]
key_files:
  created: []
  modified:
    - src/channel/webhook.rs
    - mobile/lib/services/api_service.dart
    - src/main.rs
decisions:
  - "JWT decode in resolve_owner_or_401 with owner_map static fallback for backward compat"
  - "OtcStore exposed as pub type alias + new_otc_store() factory; serve_with_mesh accepts it as param"
  - "WR-03: unified 401 body for expired vs unknown OTC prevents enumeration oracle"
metrics:
  duration: "~8 minutes"
  completed: "2026-06-18T00:29:52Z"
  tasks_completed: 2
  tasks_total: 2
  files_modified: 3
---

# Phase 06 Plan 05: Mobile Auth Chain Fix Summary

JWT decode wired into resolve_owner_or_401 (HS256 + owner_map fallback); OtcStore exposed for skill command writes; Flutter sendMessage contract aligned to {'text'}/{reply}.

## Tasks Completed

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | Decode JWT in resolve_owner_or_401 + fail-closed on missing secret (CR-01, WR-01) | af46086 | src/channel/webhook.rs |
| 2 | Expose OtcStore write path + align Flutter sendMessage contract (CR-02, CR-05, WR-03) | 5113b8b | src/channel/webhook.rs, mobile/lib/services/api_service.dart, src/main.rs |

## What Was Built

**CR-01 fixed:** `resolve_owner_or_401` now calls `jsonwebtoken::decode::<Claims>` with HS256 validation. If the JWT decodes successfully, `claims.sub` is returned as `owner_id`. Falls back to static `owner_map` lookup for pre-existing CLI/API tokens (backward compat). All 4 callers updated to pass `&state.jwt_secret`.

**WR-01 fixed:** `WebhookChannel::run` uses `.map_err(|_| anyhow::anyhow!(...))` on `std::env::var("APP_JWT_SECRET")` — refuses to start with a tracing::error log instead of silently using a hardcoded default.

**CR-02 fixed:** `OtcStore` public type alias + `new_otc_store()` factory function exported. `serve_with_mesh` accepts `OtcStore` as last param. `serve()` shim passes `new_otc_store()`. `main.rs` creates a store and passes it — skill commands can now hold a clone and insert BAST-XXXX codes.

**CR-05 fixed:** Flutter `sendMessage` sends `{'text': message}` (not `{'message':}`) and reads `resp.data['reply']` (not `resp.data['response']`). Matches Rust `struct In { text }` / `struct Out { reply }` exactly.

**WR-03 fixed:** Expired OTC in `auth_exchange_handler` and `mesh_pair_handler` now returns the same 401 body as unknown OTC (`{"error": "invalid OTC"}` / `{"error": "invalid pairing token"}`). Distinction logged server-side only.

**Module-level Claims struct:** `#[derive(Serialize, Deserialize)]` at module scope; inner duplicate in `auth_exchange_handler` removed.

## Tests Added

| Test | Behavior |
|------|----------|
| `post_webhook_valid_jwt_returns_200` | Valid HS256 JWT → 200 on /webhook |
| `post_webhook_jwt_wrong_key_returns_401` | JWT signed with wrong key → 401 |
| `post_webhook_expired_jwt_returns_401` | Expired JWT (exp in past) → 401 |
| `post_webhook_raw_non_jwt_returns_401` | Non-JWT string not in owner_map → 401 |
| `post_webhook_static_owner_map_token_still_works` | Static "token-mario" still resolves → 200 |
| `post_auth_exchange_valid_otc_returns_jwt` | Freshly inserted OTC → 200 + {jwt, device_name} |
| `new_otc_store_is_accessible` | new_otc_store() returns usable store for callers |

Total: 211 tests pass (was 193 before this plan; +18 in webhook module).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocker] main.rs caller of serve_with_mesh had wrong arity**
- **Found during:** Task 2 implementation
- **Issue:** `main.rs:252` called `serve_with_mesh` without the new `otc_store` parameter, causing compile error
- **Fix:** Added `new_otc_store()` creation in main.rs and passed it as the final argument
- **Files modified:** src/main.rs
- **Commit:** 5113b8b

**2. [Rule 2 - Missing] main.rs still had old APP_JWT_SECRET fallback comment**
- **Found during:** Task 2 review
- **Issue:** The comment was misleading — now correctly notes the webhook channel enforces fail-closed
- **Fix:** Updated comment in main.rs for clarity
- **Files modified:** src/main.rs
- **Commit:** 5113b8b

## Threat Surface Scan

All changes directly address threats in the plan's `<threat_model>`:

| Threat | Status |
|--------|--------|
| T-06-05-01: Spoofing via unverified JWT (CR-01) | MITIGATED — jsonwebtoken::decode + HS256 Validation |
| T-06-05-02: Auth bypass via hardcoded secret (WR-01) | MITIGATED — fail-closed on missing APP_JWT_SECRET |
| T-06-05-03: OTC enumeration via differential response (WR-03) | MITIGATED — unified 401 body |
| T-06-05-04: sendMessage contract mismatch (CR-05) | RESOLVED — keys aligned |

No new network endpoints, auth paths, or trust boundaries introduced.

## Known Stubs

None. All plan deliverables are fully wired:
- JWT decode path is live in all protected routes
- OtcStore is wired in main.rs (though no skill command inserts OTCs yet — that's a future plan scope, CR-02 fix is the infrastructure half)
- sendMessage contract is correct

## Self-Check: PASSED

- [x] src/channel/webhook.rs exists with `jsonwebtoken::decode`, `APP_JWT_SECRET must be set`, `OtcStore`, `new_otc_store`
- [x] mobile/lib/services/api_service.dart has `'text': message` and `resp.data['reply']`
- [x] Commits af46086 and 5113b8b exist
- [x] `cargo build -p bastion` exits 0
- [x] `cargo test -p bastion` 211 passed
