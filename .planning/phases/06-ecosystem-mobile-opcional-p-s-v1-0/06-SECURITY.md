---
phase: 06-ecosystem-mobile-opcional-p-s-v1-0
slug: ecosystem-mobile-opcional-p-s-v1-0
status: verified
threats_total: 41
threats_closed: 41
threats_open: 0
asvs_level: 1
created: 2026-06-18
---

# Phase 06 — Security Audit

> Per-phase security contract: threat register, accepted risks, and audit trail.

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| peer daemon → /mesh/ingest | Untrusted HTTP POST from remote Bastion instance | Encrypted MeshEnvelope (age ciphertext) |
| Flutter app → /events | SSE subscription on LAN | Real-time turn events (JWT-gated) |
| filter_for_mesh output → MeshTransport::send | Filtered belief slice crosses node boundary | CloudOk beliefs only; LocalOnly stripped |
| age ciphertext → relay | Relay forwards opaque blob | Vec<u8> ciphertext only; relay holds no key |
| /auth/exchange OTC → JWT | Short-lived code exchanged for long-lived token | OTC (5-min TTL), JWT (90-day) |
| /mesh/pair body → peer registration | Attacker-supplied peer_url + age_pubkey written to config | URL + key material |
| bastion.toml config write | Peer-supplied values persisted | TOML-structured peer entry |
| SQLite beliefs table → filter_for_mesh | privacy_tier column gates mesh egress | Belief content + tier |
| mobile app → /webhook | JWT-authenticated HTTP from Flutter | Chat messages |
| flutter_secure_storage → OS keystore | iOS Keychain / Android Keystore | JWT token |

---

## Threat Register

| Threat ID | Category | Component | Disposition | Mitigation | Status | Evidence |
|-----------|----------|-----------|-------------|------------|--------|---------|
| T-06-01-01 | Spoofing | /mesh/ingest from_owner field | mitigate | ingest_handler returns 501 until Plan 02 wires receive(); no from_owner trusted | closed | `src/channel/webhook.rs` — stub replaced by transport.receive() in Plan 02; from_owner verified in p2p.rs:153 |
| T-06-01-02 | Information Disclosure | GET /events SSE without auth | mitigate | resolve_owner_or_401() applied to sse_handler before BroadcastStream subscription | closed | `src/channel/webhook.rs:220` — sse_handler calls resolve_owner_or_401 with jwt_secret |
| T-06-01-03 | Information Disclosure | LocalOnly belief leaks to mesh | mitigate | filter_for_mesh two-stage: tag allowlist + check_egress(tier, "mesh") — LocalOnly returns false at stage 2 | closed | `src/mesh/allowlist.rs:38` — check_egress called per belief; 5 unit tests assert invariant |
| T-06-01-04 | Information Disclosure | Relay reads ciphertext content | mitigate | MeshEnvelope.ciphertext typed Vec<u8> — opaque by construction; relay holds no private key | closed | `src/mesh/mod.rs` — MeshEnvelope.ciphertext: Vec<u8>; P2PTransport::send encrypts before sending |
| T-06-01-05 | Tampering | filter_for_mesh called after send | mitigate | Call order enforced: filter_for_mesh → check_egress → age::encrypt → send; MeshTransport::send receives pre-filtered slice | closed | `src/scheduler/cron.rs:128` — filter_for_mesh called before transport.send; `src/mesh/p2p.rs` — caller responsibility documented in trait comment |
| T-06-01-06 | Elevation of Privilege | age dep supply chain | accept | age 0.11.x is the FiloSottile reference Rust implementation, actively maintained | closed | Accepted risk — see Accepted Risks Log (AR-01) |
| T-06-01-07 | Spoofing | /auth/exchange OTC brute force | mitigate | OTC has 5-min TTL; single-use (consumed on first use); random UUID entropy | closed | `src/channel/webhook.rs:327-344` — elapsed.as_secs() < 300; remove() on use |
| T-06-01-08 | Elevation of Privilege | /mesh/pair with forged pairing token | mitigate | Pairing token 5-min TTL, single-use; x-bastion-token required on /mesh/pair | closed | `src/channel/webhook.rs:404,418,420` — resolve_owner_or_401 enforced; TTL checked; token removed on success |
| T-06-02-01 | Spoofing | from_owner mismatch after age::decrypt | mitigate | receive() checks envelope.from_owner != slice.from_owner → bail! | closed | `src/mesh/p2p.rs:153-157` — mismatch → bail with descriptive error |
| T-06-02-02 | Spoofing | Unregistered peer sends /mesh/ingest | mitigate | receive() calls peers.resolve(&slice.from_owner).is_none() → bail! | closed | `src/mesh/p2p.rs:170-174` — unregistered peer rejected |
| T-06-02-03 | Tampering / Elevation | Remote slice parsed by AgentLoop | mitigate | SEAM #2 rule: ContextBlock.content is a String; AgentLoop includes verbatim, never parses | closed | `src/mesh/context_provider.rs:37-50` — content formatted as plain String; loop_.rs includes it as opaque block |
| T-06-02-04 | Information Disclosure | Cabinet synthesis contains LocalOnly data | mitigate | write_cabinet_synthesis() always stores with CloudOk tier | closed | `src/mesh/context_provider.rs:118` — Some(PrivacyTier::CloudOk) explicit |
| T-06-02-05 | Denial of Service | age::decrypt with malformed ciphertext panics | mitigate | age::decrypt returns Result; ? propagation → ingest_handler returns 400, daemon continues | closed | `src/mesh/p2p.rs:145-148` — map_err + ? propagation; `src/channel/webhook.rs:284-290` — Err → 400 |
| T-06-02-06 | Server Side Request Forgery | peer_url from bastion.toml in reqwest::post | accept | peer_url is operator-configured (not user-controlled during send); mesh sync only calls registered peers | closed | Accepted risk — see Accepted Risks Log (AR-02). Note: user-supplied peer_url at /mesh/pair is separately SSRF-blocked (T-06-07-05) |
| T-06-02-07 | Information Disclosure | mesh_cabinet_synthesis shared to wrong peer | mitigate | filter_for_mesh requires "mesh_cabinet_synthesis" in peer's allowed_tags; not listed → filtered out | closed | `src/mesh/allowlist.rs:31-34` — tag allowlist stage gates all beliefs including synthesis |
| T-06-02-08 | Information Disclosure | CronService sends unfiltered beliefs | mitigate | spawn_mesh_sync_job calls filter_for_mesh before transport.send | closed | `src/scheduler/cron.rs:128` — filter_for_mesh called per peer before send |
| T-06-03-01 | Information Disclosure | JWT stored in SharedPreferences (plaintext) | mitigate | FlutterSecureStorage enforced; no SharedPreferences anywhere | closed | `mobile/lib/services/api_service.dart:12` — FlutterSecureStorage; grep confirms no SharedPreferences in mobile/lib/ |
| T-06-03-02 | Information Disclosure | SSE /events without x-bastion-token | mitigate | SseService sets x-bastion-token header; 401 → onAuthExpired → PairingScreen | closed | `mobile/lib/services/sse_service.dart:46-49` — header injected; `sse_service.dart:62-64` — 401 triggers re-pair |
| T-06-03-03 | Elevation of Privilege | OTC intercepted over HTTP (non-TLS LAN) | accept | Default is LAN HTTP (MVP); Bastion Cloud mandates TLS; Tailscale recommended for remote | closed | Accepted risk — see Accepted Risks Log (AR-03) |
| T-06-03-04 | Denial of Service | SSE reconnection loop floods daemon | mitigate | Exponential backoff: 1s → 2s → 4s → 8s → max 60s (capped at 1 req/min after 6 retries) | closed | `mobile/lib/services/sse_service.dart:88` — min(1 << _retryCount, 60) |
| T-06-03-05 | Tampering | SSE event data injected into UI as executable | accept | Flutter Text widget renders strings as plain text; no eval path | closed | Accepted risk — see Accepted Risks Log (AR-04) |
| T-06-03-06 | Information Disclosure | Cockpit screen in app task-switcher screenshot | accept | OS-level concern; FLAG_SECURE (Android) is V2 concern; documented known limitation | closed | Accepted risk — see Accepted Risks Log (AR-05) |
| T-06-03-07 | Tampering | ContestableMemoryView sends arbitrary belief ID | mitigate | IDs parsed from daemon's own /memories response (not user-entered); daemon validates ownership before contesting | closed | `mobile/lib/screens/cockpit_screen.dart:807` — sendMessage('/contest $beliefId') where beliefId comes from daemon response |
| T-06-04-01 | Tampering | agentskills-install path traversal in name | mitigate | Install skill validates name; rejects names with .., /, \; confirmation before write | closed | `skills/agentskills-install/SKILL.md:57-71` — path traversal section; `SKILL.md:93` — confirmation pattern |
| T-06-04-02 | Information Disclosure | agentskills-publish exposes hardcoded secrets | mitigate | Publish skill has explicit security note: check for secrets; only cloud-ok tier skills | closed | `skills/agentskills-publish/SKILL.md` — security note section present |
| T-06-04-03 | Elevation of Privilege | community channel omits OwnerMap.resolve auth | mitigate | channel-scaffold documents CR-03 requirement as mandatory step with code example | closed | `skills/channel-scaffold/SKILL.md:72-88` — Rule 4: CR-03 mandatory for HTTP handlers |
| T-06-04-04 | Spoofing | ClawHub skill missing privacy_tier defaults to unrestricted | mitigate | ClawHub migration doc notes: missing privacy_tier → defaults to CloudOk; advises explicit tier | closed | `docs/clawhub-migration.md` — compatibility notes section documents default behavior |
| T-06-04-05 | Tampering | Installed SKILL.md contains malicious instructions | accept | SKILL.md is a definition file; Bastion reads but does not exec code from it | closed | Accepted risk — see Accepted Risks Log (AR-06) |
| T-06-05-01 | Spoofing | resolve_owner_or_401 JWT not decoded | mitigate | CR-01: jsonwebtoken::decode HS256; tampered/expired tokens rejected | closed | `src/channel/webhook.rs:171` — decode::<Claims> with Validation::new(HS256); sub claim returned as owner_id |
| T-06-05-02 | Elevation of Privilege | APP_JWT_SECRET default fallback | mitigate | WR-01: fail-closed — anyhow bail! when env var unset; never signs with hardcoded default | closed | `src/channel/webhook.rs:50-56` — map_err bails if APP_JWT_SECRET unset |
| T-06-05-03 | Spoofing | OTC enumeration oracle | mitigate | WR-03: identical 401 body for expired vs unknown OTC; distinction logged server-side only | closed | `src/channel/webhook.rs:345-355` — both branches return {"error":"invalid OTC"} |
| T-06-05-04 | Tampering | sendMessage contract key mismatch | accept | CR-05: correctness defect (422), not a security issue; fix aligns contract | closed | Accepted risk — see Accepted Risks Log (AR-07). Fixed: api_service.dart:51-53 uses {text}/{reply} |
| T-06-06-01 | Information Disclosure | retrieve_tagged returning tier:None | mitigate | CR-03: SELECT privacy_tier; map to Some(tier); None = DB NULL = deny-on-ambiguity preserved | closed | `src/memory/sqlite.rs:81,137` — privacy_tier in SELECT; `sqlite.rs:143-147` — mapped to Option<PrivacyTier> |
| T-06-06-02 | Information Disclosure | write_cabinet_synthesis with no tier | mitigate | CR-04: Some(CloudOk) passed explicitly — no implicit tier promotion | closed | `src/mesh/context_provider.rs:118` — Some(crate::memory::PrivacyTier::CloudOk) |
| T-06-06-03 | Tampering | ALTER TABLE migration on existing DB | accept | ADD COLUMN is additive/idempotent; existing rows get NULL (deny) which is the safe default | closed | Accepted risk — see Accepted Risks Log (AR-08). `src/session/sqlite.rs:98` — let _ = (ignores duplicate column error) |
| T-06-07-01 | Elevation of Privilege | P2PTransport::receive() missing to_owner check | mitigate | CR-06: assert envelope.to_owner == self.local_owner; bail with descriptive error | closed | `src/mesh/p2p.rs:163-167` — envelope.to_owner != self.local_owner → bail! |
| T-06-07-02 | Elevation of Privilege | ingest_handler missing to_owner guard | mitigate | CR-06: check MESH_OWNER_ID/BASTION_OWNER_ID env var vs envelope.to_owner; 403 on mismatch | closed | `src/channel/webhook.rs:263-274` — belt-and-suspenders guard; primary enforcement unconditional in p2p.rs:163. Note: ingest guard fires only when env var is set; see WARNING below. |
| T-06-07-03 | Tampering | append_mesh_peer TOML injection via format!() | mitigate | SEC-01: toml_edit programmatic construction; validate_age_pubkey ^age1[0-9a-z]+$ regex; bail on read error | closed | `src/config.rs:124-191` — toml_edit; validate_age_pubkey at line 140; bail on read error at line 149 |
| T-06-07-04 | Tampering | append_mesh_peer overwrites config on read error | mitigate | WR-02: bail! on read error; atomic write via temp-file + rename | closed | `src/config.rs:147-149` — map_err bail on read_to_string; `config.rs:185-187` — .tmp + rename |
| T-06-07-05 | Server-Side Request Forgery | mesh_pair_handler unvalidated peer_url | mitigate | SEC-02: url::Url parse; https-only; DNS-resolve + is_private_ip (RFC1918/loopback/link-local/ULA/IPv4-mapped/deprecated site-local); redirect::Policy::none() | closed | `src/channel/webhook.rs:431-464` — full SSRF block; `webhook.rs:359-385` — is_private_ip covers all RFC classes + IPv6; `src/mesh/p2p.rs:45` — redirect::Policy::none() |
| T-06-07-06 | Information Disclosure | allowed_tags dropped on re-pair | mitigate | WR-02: toml_edit preserves existing entries including allowed_tags; atomic write | closed | `src/config.rs:153-182` — toml_edit doc parse → append → write; existing entries preserved |

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-01 | T-06-01-06 | age 0.11.x is the FiloSottile reference Rust implementation (actively maintained, audited). No practical X25519+ChaCha20-Poly1305 alternative. Supply chain risk is low. | Plan author | 2026-06-17 |
| AR-02 | T-06-02-06 | peer_url used during MeshTransport::send is operator-configured in bastion.toml, not user-controlled at send time. Operator controls their own config. Note: user-supplied peer_url at /mesh/pair is separately SSRF-blocked by T-06-07-05 mitigation. | Plan author | 2026-06-17 |
| AR-03 | T-06-03-03 | Default LAN HTTP for MVP. Bastion Cloud (closed) mandates TLS. Tailscale/WireGuard overlay documented as recommendation for remote access over untrusted networks. | Plan author | 2026-06-17 |
| AR-04 | T-06-03-05 | Flutter Text widget renders strings as plain text only; no eval() or innerHTML-equivalent path exists in Flutter. Risk is framework-structural, not code-level. | Plan author | 2026-06-17 |
| AR-05 | T-06-03-06 | OS-level task-switcher screenshot is handled by FLAG_SECURE (Android) / UIScreen.ignoreSnapshotOnNextApplicationLaunch (iOS). Documented as V2 known limitation per threat register. | Plan author | 2026-06-17 |
| AR-06 | T-06-04-05 | SKILL.md is a definition/instruction file. Bastion reads it to understand skill intent but never eval()'s code within it. The skill-writer pattern (read → confirm → write) is the only execution path; a malicious SKILL.md can misdirect the agent but cannot exec arbitrary code in the runtime. | Plan author | 2026-06-17 |
| AR-07 | T-06-05-04 | CR-05 is a correctness defect (422 Unprocessable Entity), not a security vulnerability. The mismatch between {message}/{response} and {text}/{reply} is fixed in Plan 05. Accepted as a correctness issue, not a security risk. | Plan author | 2026-06-17 |
| AR-08 | T-06-06-03 | ALTER TABLE ADD COLUMN is additive and idempotent in SQLite. Existing rows receive NULL for privacy_tier, which is the correct deny-on-ambiguity default (check_egress(None, ...) returns Err). No data loss; migration is safe on both fresh and existing DBs. | Plan author | 2026-06-18 |

*Accepted risks do not resurface in future audit runs.*

---

## Unregistered Threat Flags

The following flags were noted in SUMMARY.md files during implementation. All map to existing threat IDs or are informational:

| Flag Source | Description | Maps To |
|-------------|-------------|---------|
| 06-02-SUMMARY.md § Threat Flags | "No new surfaces beyond those documented in the plan's threat model. All 8 STRIDE threats mitigated as planned." | All T-06-02-* — informational |
| 06-06-SUMMARY.md § Threat Flags | "No new security surface introduced. Additive migration gives existing rows NULL tier = safe deny-on-ambiguity default." | T-06-06-03 (accept) — informational |

No unregistered flags (no new attack surface without a threat mapping).

---

## Warnings (Non-Blocking)

| Warning ID | Threat Ref | Description | Recommendation |
|------------|------------|-------------|----------------|
| W-01 | T-06-07-02 | The ingest_handler `to_owner` guard (webhook.rs:263) is conditional on `MESH_OWNER_ID` or `BASTION_OWNER_ID` env var being set. If neither is set, the 403 belt-and-suspenders layer does not fire. The primary enforcement (P2PTransport::receive() at p2p.rs:163) is unconditional and cannot be bypassed. Defense-in-depth is weakened but not absent. | Set MESH_OWNER_ID or BASTION_OWNER_ID in .env when mesh is enabled. Consider promoting the guard to always-on by storing local_owner in AppState. |

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | ASVS Level | Run By |
|------------|---------------|--------|------|------------|--------|
| 2026-06-18 | 41 | 41 | 0 | 1 | gsd-security-auditor (Claude Sonnet 4.6) |

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-06-18
