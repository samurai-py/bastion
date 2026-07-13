// Memory backends. The `Memory` trait + `SharedMemory` alias moved to
// `bastion_runtime::memory` (M2 step 3b, decision D1 — the runtime defines
// the port, backends here implement it) and the pure data types in its
// signatures moved to `bastion-types`. Everything is re-exported here so
// every existing `crate::memory::...` path keeps compiling unchanged.
// SqliteMemory backend is in sqlite.rs (becomes `bastion-memory` in M2 step 4).
// Tests (offline, temp DB) are in sqlite.rs #[cfg(test)].

pub use bastion_runtime::memory::{Memory, SharedMemory};
pub use bastion_types::{Belief, BeliefDraft, BeliefKind, Outcome, PendingCorrection, PrivacyTier};

pub mod sqlite;
