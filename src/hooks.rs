// TEMPORARY re-export shim (M2). Remove by end of M3 (docs/revamp/M1-ADR-substrate-split.md).
//! Shim (M2 step 3b): the runtime hooks moved to `bastion_runtime::hooks`.
//! Re-exported here so every existing `crate::hooks::...` path keeps
//! compiling unchanged.

pub use bastion_runtime::hooks::{approval_intent, egress, guardrails, observer, output_validator};
pub use bastion_runtime::hooks::{Hook, NoObserver, Observer};
