//! Shim (M2 step 6): the offline learning Reflector (LEARN-02..05, delta/dedup)
//! moved to `bastion-cognition`. Re-exported here so every existing
//! `bastion::learn::...` path keeps compiling unchanged.

pub use bastion_cognition::learn::*;
