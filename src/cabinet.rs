//! Shim (M2 step 6): the Cabinet multi-persona deliberation engine (stable
//! contract, decision #6) moved to `bastion-cognition`. Re-exported here so
//! every existing `bastion::cabinet::...` path keeps compiling unchanged.

pub use bastion_cognition::cabinet::*;
