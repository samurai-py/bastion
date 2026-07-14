// TEMPORARY re-export shim (M2). Remove by end of M3 (docs/revamp/M1-ADR-substrate-split.md).
//! Shim (M2 step 3b): the session store moved to `bastion_runtime::session`.
//! Re-exported here so every existing `crate::session::...` path keeps
//! compiling unchanged.

pub use bastion_runtime::session::sqlite;
pub use bastion_runtime::session::SessionManager;
