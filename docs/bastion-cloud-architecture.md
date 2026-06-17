# Bastion Cloud Architecture

**Version:** 1.0 · **Phase:** ECO-03 · **Decision:** D-10 / D-02

Bastion Cloud is the managed relay implementation of the `MeshTransport` trait.
This document defines the OSS/closed boundary and the architecture of the relay layer.
**No relay implementation is shipped in this OSS release (D-10).**

---

## Purpose

The Bastion OSS mesh (Phase 6, Plan 02) supports P2P sync over LAN and Tailscale — both
require direct reachability between instances. Bastion Cloud provides a managed relay so
instances behind NAT or mobile networks can sync without exposing public IPs.

The relay is a closed product built against the OSS `MeshTransport` trait. The trait is the
contract; the implementation is separate.

---

## OSS vs. Closed Boundary

| Component | Repo | Ships in v1.0? |
|---|---|---|
| `MeshTransport` trait (`src/mesh/mod.rs`) | OSS | Yes — trait boundary defined |
| `P2PTransport` impl (`src/mesh/p2p.rs`) | OSS | Yes — direct HTTP/age E2E |
| Relay implementation | Closed (Bastion Cloud) | No — separate repo, future phase |
| NAT traversal / STUN | Closed | No |
| Store-and-forward | Closed | No |
| Multi-tenant routing | Closed | No |
| Discovery service | Closed | No |

The relay implements `MeshTransport` in a closed repository. The OSS daemon loads the trait;
the specific `impl` (P2P vs. relay) is injected at startup via `SharedMeshTransport`.

---

## Architecture Diagram

```
┌──────────────┐        ┌──────────────────────────────────────────┐
│  Flutter App │        │          Bastion Daemon (OSS)            │
│  (iOS/Android)│       │                                          │
│              │──JWT──▶│  /auth/exchange  /events (SSE)          │
│              │◀───────│  /webhook  /mesh/ingest                  │
└──────────────┘        │                                          │
                        │  AgentLoop ──▶ AgentHandle               │
                        │  MeshTransport (trait, pluggable)        │
                        └──────────┬───────────────────────────────┘
                                   │
                    ┌──────────────┴──────────────┐
                    │                             │
          [OSS path: P2P]              [Cloud path: relay]
                    │                             │
          direct HTTPS POST            HTTPS POST to relay
          /mesh/ingest on peer         (closed Bastion Cloud)
                    │                             │
         ┌──────────▼──────────┐        ┌────────▼────────┐
         │   Peer Bastion      │        │  Bastion Cloud  │
         │   Daemon (OSS)      │        │  Relay (closed) │
         │   /mesh/ingest      │        │  blind forward  │
         └─────────────────────┘        └────────┬────────┘
                                                 │
                                        ┌────────▼────────┐
                                        │   Peer Bastion  │
                                        │   Daemon (OSS)  │
                                        │   /mesh/ingest  │
                                        └─────────────────┘
```

**OSS path:** daemon calls `P2PTransport.send()` → HTTP POST to `peer_url/mesh/ingest`
directly. Requires both peers to be mutually reachable (LAN or Tailscale).

**Cloud path:** daemon calls relay `MeshTransport.send()` → HTTPS POST to Bastion Cloud relay
→ relay stores the opaque ciphertext and forwards to the peer's pull endpoint or pushes via
SSE. Peer daemon calls `relay.receive()` → decrypts locally.

---

## Relay is Blind

The relay never holds a private key and never reads belief content.

```rust
/// Opaque wire envelope — ciphertext is E2E encrypted with `age`.
/// The relay (closed Bastion Cloud) forwards this blob without reading it.
/// `ciphertext: Vec<u8>` is opaque by type — relay never holds the private key.
pub struct MeshEnvelope {
    pub from_owner: String,
    pub to_owner:   String,
    /// age-encrypted serialized SelectiveSlice — opaque to relay.
    pub ciphertext:       Vec<u8>,
    /// age recipient public key hint (bech32). Used by receiver to select decryption key.
    pub recipient_hint:   String,
}
```

The `ciphertext` field is `Vec<u8>` — the relay receives it as opaque bytes and forwards
them. It cannot decrypt without the recipient's age private key, which never leaves the
recipient's daemon. This is enforced by type: the relay's ingest handler has no reference
to any private key.

The relay operates as a "land-and-expand" store-and-forward: it holds the ciphertext only
until the peer pulls or a push succeeds. No persistent storage of belief content.

---

## MeshTransport Trait Contract

The OSS trait defines the interface that both `P2PTransport` (OSS) and the relay (closed) must implement:

```rust
// src/mesh/mod.rs
#[async_trait::async_trait]
pub trait MeshTransport: Send + Sync {
    /// Send a selective slice to a remote owner.
    /// The slice MUST already have LocalOnly beliefs filtered out by filter_for_mesh.
    /// Implementor encrypts with the peer's age public key before transit.
    async fn send(&self, slice: SelectiveSlice, to_owner: &str) -> anyhow::Result<()>;

    /// Receive an incoming envelope (called by /mesh/ingest handler after auth).
    /// Implementor decrypts and verifies from_owner against registered peer keys.
    async fn receive(&self, envelope: MeshEnvelope) -> anyhow::Result<SelectiveSlice>;
}

pub type SharedMeshTransport = std::sync::Arc<dyn MeshTransport>;
```

Both `send` and `receive` operate on `SelectiveSlice` (plaintext, pre-filtered) and
`MeshEnvelope` (ciphertext, on-wire). Encryption/decryption is the implementor's responsibility
and happens inside the transport, not in the caller.

---

## Privacy Invariants (Both Paths)

These invariants hold regardless of which transport is active:

1. **WR-04 enforced before transport**: `filter_for_mesh` calls `check_egress` on every belief.
   `LocalOnly` beliefs are dropped before they reach `MeshTransport.send()`. The relay never
   sees them.

2. **Allowlist enforced before transport**: `filter_for_mesh` applies the peer's `allowed_tags`
   filter. Only explicitly whitelisted tags cross the boundary.

3. **E2E encryption**: ciphertext is produced by the sender's daemon using the peer's age key.
   The relay (and any network hop) sees only opaque bytes.

4. **Relay cannot correlate content**: because the relay is blind to plaintext, it cannot learn
   which beliefs are shared between which owners — only that an envelope was routed from
   `from_owner` to `to_owner`. Metadata minimization (hiding owner IDs from the relay) is a
   future closed-repo enhancement.

---

## Phase 6 Deliverable

This phase ships:

- `MeshTransport` trait boundary defined in `src/mesh/mod.rs` (OSS)
- `P2PTransport` implementation for direct mesh (OSS, `src/mesh/p2p.rs`)
- This architecture document (`docs/bastion-cloud-architecture.md`)

The relay does **not implement** the relay transport in OSS. The closed Bastion Cloud relay is
a separate product that implements `MeshTransport` against this trait contract. OSS users who
need cross-network sync without Tailscale will be able to plug in the relay as a `[[mesh.transport]]`
config entry in a future phase.

---

## Future Phases

| Phase | Deliverable |
|---|---|
| Closed v1 | Relay MVP: store-and-forward, multi-tenant, HTTPS only |
| Closed v2 | NAT traversal (STUN/TURN), discovery service |
| OSS (future) | `[[mesh.transport]] type = "relay"` config + relay auth flow |
| OSS (future) | Metadata minimization (sender/receiver anonymity from relay) |
