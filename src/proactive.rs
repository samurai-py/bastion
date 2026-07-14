//! Shim (M2 step 6): the proactive heartbeat/idle scheduler (PROACT-01..05)
//! moved to `bastion-cognition`. Re-exported here so every existing
//! `bastion::proactive::...` path keeps compiling unchanged.

pub use bastion_cognition::proactive::*;
