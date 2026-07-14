//! Shim (M2 step 6): agent identity (`AgentCard` + Ed25519 signing, SEC-06)
//! moved to `bastion-mesh`. Re-exported here so every existing
//! `bastion::identity::...` path keeps compiling unchanged.

pub use bastion_mesh::identity::*;
