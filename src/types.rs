// TEMPORARY re-export shim (M2). Remove by end of M3 (docs/revamp/M1-ADR-substrate-split.md).
//! Shim (M2 step 2): leaf types moved to `bastion-types`. Re-exported here so
//! every existing `bastion::types::...` path keeps compiling unchanged.

pub use bastion_types::*;
