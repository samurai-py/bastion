// TEMPORARY re-export shim (M2). Remove by end of M3 (docs/revamp/M1-ADR-substrate-split.md).
//! Shim (M2 step 6): the goal engine (GOAL-01..03) moved to
//! `bastion-cognition`. Re-exported here so every existing
//! `bastion::goal::...` path keeps compiling unchanged.

pub use bastion_cognition::goal::*;
