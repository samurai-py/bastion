---
phase: 06-ecosystem-mobile-opcional-p-s-v1-0
plan: "04"
subsystem: ecosystem-skills-docs
tags:
  - skills
  - agentskills
  - clawhub
  - bastion-cloud
  - mesh
  - channel-scaffold
  - eco-01
  - eco-02
  - eco-03
  - chex-01
  - chex-02
  - chex-03
dependency_graph:
  requires:
    - 06-01 (MeshTransport trait + /mesh/pair endpoint)
    - 06-02 (P2PTransport, filter_for_mesh, write_cabinet_synthesis, spawn_mesh_sync_job)
  provides:
    - skills/agentskills-publish/SKILL.md (ECO-01 publish pipeline)
    - skills/agentskills-install/SKILL.md (ECO-01 install-by-conversation)
    - skills/mesh-connect/SKILL.md (P2P pairing wizard, MESH support)
    - skills/channel-scaffold/SKILL.md (CHEX-01/02/03 template)
    - docs/clawhub-migration.md (ECO-02 ClawHub migration path)
    - docs/bastion-cloud-architecture.md (ECO-03 Bastion Cloud doc)
  affects: []
tech_stack:
  added: []
  patterns:
    - skill-writer conversational flow (step-by-step flow table)
    - mobile-connect pairing wizard (analog for mesh-connect /connect-peer)
    - skills-ref validate as mandatory validation step for ECO-01 publish
key_files:
  created:
    - skills/agentskills-publish/SKILL.md
    - skills/agentskills-install/SKILL.md
    - skills/mesh-connect/SKILL.md
    - skills/channel-scaffold/SKILL.md
    - docs/clawhub-migration.md
    - docs/bastion-cloud-architecture.md
  modified: []
decisions:
  - "D-08 DELIVERED: agentskills-publish covers Bastion→hub direction; agentskills-install covers install/search-by-conversation direction on existing hybrid SKILL.md"
  - "D-09 DELIVERED: ClawHub migration documented with frontmatter rename table + skills-ref validate + worked reminder skill example"
  - "D-10 DELIVERED: Bastion Cloud architecture doc captures MeshTransport trait boundary; relay implementation NOT shipped in OSS"
  - "D-11 DELIVERED: only /add-<channel> scaffold skill + doc ship; specific channels (WhatsApp/Discord/Email) are community/future work"
metrics:
  duration: "~25 minutes"
  completed: "2026-06-17"
  tasks_completed: 2
  tasks_total: 2
  files_created: 6
  files_modified: 0
---

# Phase 6 Plan 04: Ecosystem Skills + Docs Summary

All skill-layer and doc-layer Phase 6 deliverables in a single plan. ECO-01/02/03 + CHEX-01/02/03 complete. No Rust changes.

---

## Tasks

| # | Name | Commit | Files |
|---|------|--------|-------|
| 1 | ECO skills + mesh-connect skill | f408248 | skills/agentskills-publish/SKILL.md, skills/agentskills-install/SKILL.md, skills/mesh-connect/SKILL.md |
| 2 | channel-scaffold skill + ClawHub doc + Bastion Cloud arch doc | 32a7465 | skills/channel-scaffold/SKILL.md, docs/clawhub-migration.md, docs/bastion-cloud-architecture.md |

---

## What Was Built

### ECO-01: agentskills-publish + agentskills-install

`agentskills-publish` handles Pitfall 4: existing skills use `name: bastion/<slug>`, but agentskills.io
rejects names with slashes. The skill strips the prefix, preserves `metadata.bastion_name: bastion/<slug>`
for reinstall mapping, runs `skills-ref validate`, and guides the git push flow. Security notes cover
secret hygiene and `privacy_tier: cloud-ok` requirement before publishing.

`agentskills-install` handles the reverse: fetch SKILL.md from bare name (via agentskills.io index),
GitHub URL, or direct URL; validate the `name` field for path traversal (`..`, `/`, `\` rejection per T-06-04-01);
show confirmation before writing; warn on `cloud-only` privacy tier.

### ECO-01: mesh-connect

Documents the full P2P pairing wizard against the real `/mesh/pair` endpoint in `src/channel/webhook.rs`.
The 7-step `/connect-peer` flow (generate BAST-PEER-XXXX OTC token → share out-of-band → peer POSTs
`/mesh/pair { token, peer_url, age_pubkey }` → validate TTL → register in `bastion.toml [[mesh.peer]]`) maps
exactly to the implementation. Privacy guarantees reference WR-04 and `local-only` belief exclusion
by `filter_for_mesh`. Canonical Mario+Ana example shows allowlist-only sharing of `mercado/calendario`.

### ECO-02: ClawHub migration (D-09)

Migration steps: copy SKILL.md → rename frontmatter fields (ClawHub `skill_name`/`about`/`keywords`
→ Bastion `name`/`description`/`triggers`) → `skills-ref validate` → test trigger. Worked example
migrates a reminder skill end-to-end showing before/after frontmatter and the validate output.
Compatibility note: missing `privacy_tier` defaults to `cloud-ok` in Bastion — document advises
adding explicit `local-only` for personal-data skills.

### ECO-03: Bastion Cloud architecture (D-10)

OSS/closed boundary table: `MeshTransport` trait + `P2PTransport` are OSS; relay impl, NAT traversal,
store-and-forward, discovery are closed (separate Bastion Cloud repo). ASCII architecture diagram
shows OSS path (direct POST `/mesh/ingest`) and Cloud path (blind relay forward). Relay-is-blind
guarantee documented against `MeshEnvelope.ciphertext: Vec<u8>` opaque type. No relay implementation
ships in this OSS release per D-10.

### CHEX-01/02/03: channel-scaffold (D-11)

Documents all 4 mandatory Channel rules: implement `Channel` trait (both `run()` + `default_persona()`),
register via `bastion.toml [[channel]]`, route all messages through `AgentHandle` (no direct provider
calls), and apply `OwnerMap.resolve(token)` auth in every HTTP handler (CR-03). Generates a
`skills/add-<channel>/SKILL.md` stub template. Community WhatsApp example shows the pattern without
shipping implementation. Security reminder lists 4 checks for community channel review.

---

## Decisions Made

| Decision | Summary |
|---|---|
| D-08 | agentskills.io is bidirectional: publish skill (agentskills-publish) + install by conversation (agentskills-install) on existing hybrid SKILL.md format |
| D-09 | ClawHub migration = frontmatter rename + skills-ref validate; documented and validated with reminder skill example |
| D-10 | Bastion Cloud = closed relay impl of MeshTransport; Phase 6 ships only trait boundary + arch doc; relay NOT implemented in OSS |
| D-11 | Only /add-<channel> scaffold + doc ships; specific channels (WhatsApp/Discord/Email) are community/future |

---

## Deviations from Plan

None — plan executed exactly as written.

---

## Known Stubs

None. All skill files document real API endpoints and trait interfaces verified against source code.
The `mesh-connect` skill references the live `/mesh/pair` endpoint from `src/channel/webhook.rs`.

---

## Threat Surface Scan

No new network endpoints, auth paths, or schema changes introduced. All files are documentation
(SKILL.md + Markdown). No new executable surface added.

Security mitigations documented per the threat register:

| Threat ID | Mitigation Documented |
|---|---|
| T-06-04-01 | agentskills-install: reject names with `..`, `/`, `\`; confirmation before write |
| T-06-04-02 | agentskills-publish: explicit secret check + privacy_tier gate before publishing |
| T-06-04-03 | channel-scaffold: CR-03 + OwnerMap.resolve documented as mandatory rule 4 |
| T-06-04-04 | clawhub-migration.md: missing privacy_tier defaults to cloud-ok; explicit note to add local-only |
| T-06-04-05 | accepted — SKILL.md is definition only; no code execution path |

---

## Self-Check: PASSED

- skills/agentskills-publish/SKILL.md: exists (committed f408248)
- skills/agentskills-install/SKILL.md: exists (committed f408248)
- skills/mesh-connect/SKILL.md: exists (committed f408248)
- skills/channel-scaffold/SKILL.md: exists (committed 32a7465)
- docs/clawhub-migration.md: exists (committed 32a7465)
- docs/bastion-cloud-architecture.md: exists (committed 32a7465)
- All acceptance criteria verified via grep before each commit
