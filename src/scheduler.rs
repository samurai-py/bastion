// TEMPORARY re-export shim (M2). Remove by end of M3 (docs/revamp/M1-ADR-substrate-split.md).
//! Shim (M2 step 6): the periodic mesh-sync scheduler moved to
//! `bastion-mesh` (deviation from the BACKLOG topology table, which grouped
//! it under `bastion-cognition` — see `bastion_cognition::lib`'s doc comment
//! for the rationale). Re-exported here so every existing
//! `bastion::scheduler::...` path keeps compiling unchanged.

pub use bastion_mesh::scheduler::*;
