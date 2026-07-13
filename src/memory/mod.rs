// Memory backends. The `Memory` trait + `SharedMemory` alias moved to
// `bastion_runtime::memory` (M2 step 3b, decision D1 — the runtime defines
// the port, backends here implement it) and the pure data types in its
// signatures moved to `bastion-types`. The `SqliteMemory` backend (+ its
// `sqlite` module) moved to the `bastion-memory` crate (M2 step 4). Everything
// is re-exported here so every existing `crate::memory::...` path keeps
// compiling unchanged. Tests (offline, temp DB) moved with `sqlite.rs`; the
// two tests that also exercised `mesh::allowlist::filter_for_mesh` moved to
// `src/mesh/allowlist.rs` instead (memory cannot depend on mesh — see that
// file's test module for the rationale).

pub use bastion_memory::*;
