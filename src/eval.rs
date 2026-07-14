// TEMPORARY re-export shim (M2). Remove by end of M3 (docs/revamp/M1-ADR-substrate-split.md).
//! Shim (M2 step 6): the promoted eval harness (EVAL-01/EVAL-02 — failure
//! capture, regression verification, the `FailureSink` port implementation)
//! moved to `bastion-cognition`. Re-exported here so every existing
//! `bastion::eval::...` path keeps compiling unchanged.

pub use bastion_cognition::eval::*;
