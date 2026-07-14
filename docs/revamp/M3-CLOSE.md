# M3 — static subset close (2026-07-13)

> Covers the M3 items executable without external dependencies: the F1
> hardening follow-up (LOOP-REPORT.md), M3-02 (security invariants doc),
> M3-04 (examples) and the static half of M3-05 (feature flags + minimal
> build). Reference ADR: `docs/revamp/M1-ADR-substrate-split.md`.

## 1. F1 hardening — egress gated inside `ToolSource`

`ToolSource::call_tool_with_timeout` (`crates/bastion-runtime/src/agent/ports.rs`)
now takes `resolved_tier: Option<PrivacyTier>` and the production
implementation (`McpToolSource`, `crates/bastion-mcp/src/tool_source.rs`)
runs `check_egress(resolved_tier, "external")` internally BEFORE dispatching.
The two loop call sites (`dispatch_tool_loop`'s empty-registry fallback and
`run_provider_fallback`) no longer call `check_egress` themselves — same
check, same logical chokepoint, now unforgettable by construction. Covered by
two new invariant tests (`tests/characterization_boundary.rs`, map row "F1"
in `docs/revamp/M1-07-characterization-map.md`). F1 marked resolved in
`docs/revamp/LOOP-REPORT.md`.

## 2. M3-02 — security invariants reference

`docs/SECURITY-INVARIANTS.md` (public, English): the 10 BACKLOG invariants,
each with 2–4 sentences, the enforcing chokepoint (`crate::path`), and the
covering test(s), sourced from the M1-07 characterization map.

## 3. M3-04 — examples (and the API gaps they found)

`examples/minimal-agent` and `examples/embedded-host`, workspace members,
importing ONLY substrate crates (`bastion-types`/`-runtime`/`-memory`) —
never the root `bastion` package (enforced by a new `examples` CI job:
`cargo check -p minimal-agent -p embedded-host`). Both run fully offline,
exit 0.

**API gaps found (the key output — feed back into M3-01/M5):**

1. **`AgentLoop::new` hardwires its own `ApprovalQueue`** — it constructs
   `CapabilityRegistry::new().with_approval_queue(ApprovalQueue::new(db_path))`
   unconditionally; there is no constructor parameter to opt out or inject an
   alternative decision mechanism. An embedding host that wants a full turn
   cannot reach Policy 2's fail-closed "no queue" denial path at all.
2. **`ApprovalQueue` is a concrete SQLite struct, not a port** — no trait a
   second consumer can implement with its own authorization logic ("auto-deny
   over threshold", "delegate to external review"). The only lever is
   `.reject(owner, id)` on the built-in queue.
3. **A rejected approval is invisible to `invoke()`'s caller** —
   `outcome_for_existing_row` (`crates/bastion-runtime/src/capability/approval.rs`)
   maps a `Rejected` row to `ApprovalOutcome::AlreadyPending`, the same
   outcome as an undecided row. Re-invoking an explicitly denied action
   returns `Ok({awaiting_approval: true})`, never an `Err` (typed or
   otherwise). Contrast with the egress gate's `BastionError::PrivacyEgressBlocked`,
   which callers match via `downcast_ref`. A host cannot express or observe
   "this action was denied" through the public API today.

All three are demonstrated executable in `examples/embedded-host/src/main.rs`
(`demonstrate_denied_capability`, with assertions that will fail loudly if
the gap is ever closed upstream so the example gets updated).

## 4. M3-05 (static half) — feature flags + minimal build

Flags on the root app package (`Cargo.toml [features]`), default = all on
(today's exact behavior). Gates live only at composition points
(`src/main.rs`, `src/channel/mod.rs`, `src/mcp/mod.rs`) — zero `cfg` inside
`crates/*`.

| Feature | Gates | Deps removed when off |
|---|---|---|
| `channels-extra` | Discord/Slack/Email modules + spawn blocks; WhatsApp runtime wiring (module always compiles — types thread through the webhook router) | serenity, slack-morphism, rvstruct, lettre, async-imap, async-native-tls, mailparse |
| `voice` | `channel::voice` module + spawn block | cpal, hound, rustpotter (whole candle subtree), half pin |
| `mcp-server` | `mcp::server` module, `bastion mcp-stdio` subcommand, MCP-over-HTTP routes, `build_token_perms` | rmcp server-side cargo features |

**Skipped (per the >20-line-refactor rule):** a `mesh` flag — mesh types are
threaded through the webhook router's signature and handlers (~90 references
in `src/channel/webhook.rs`: `SharedMeshTransport`/`MeshSliceStore` params,
`/mesh/pair`, `/mesh/ingest`, SSE peer events). Gating it means refactoring
webhook, not adding a composition-point cfg. Candidate for the M4 product
split, where webhook itself becomes product code.

Config keys for compiled-out surfaces still parse; enabling one logs a
`*_not_compiled` warning instead of silently doing nothing.

Supported combinations: default (all on) and `--no-default-features` (min)
are the two gate-checked configurations (CI builds default; the minimal
build was verified locally with `cargo check`/`clippy -D warnings`
`--no-default-features`). Individual flags are additive and independent —
no flag requires or conflicts with another.

### Binary size (release profile: opt-level=z, fat LTO, strip)

| Build | Bytes | MB |
|---|---|---|
| Full (`cargo build --release`, default features) | 24.344.920 | 23,2 MiB (~24,3 MB) |
| Minimal (`cargo build --release --no-default-features`) | 15.592.184 | 14,9 MiB (~15,6 MB) |
| Delta | −8.752.736 | **−36,0%** |

Target "<20MB no mínimo": **met** (15,6 MB). Reference: M2-close full binary
was 24.345.624 bytes — the flags added no overhead to the default build
(−704 bytes, noise).

## 5. Gates (this close)

| Gate | Result |
|---|---|
| `cargo fmt --check` | PASS |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS (only the pre-existing `proc-macro-error2` future-incompat notice) |
| `cargo clippy -p bastion --no-default-features -- -D warnings` | PASS |
| `cargo test --workspace` (default features) | PASS — **537 passed, 0 failed** (40 suites: M2's 535 + the 2 new F1 invariant tests; 38 suites + the 2 example crates) |
| `bash scripts/check-crate-deps.sh` | PASS |
| `cargo run -p minimal-agent` / `-p embedded-host` | exit 0, offline |

## 6. Not covered here (remaining M3)

M3-01 (reduce `pub` to the contract + shim removal), M3-03 (compat tests /
API-breaking CI), M3-06 (semver/MSRV/license policy), M3-07..11
(extension protocol, conformance, manifests, auth, ContextRevision) — all
untouched by this static pass.
