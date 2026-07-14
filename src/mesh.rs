//! Shim (M2 step 6): the mesh connectivity layer moved to `bastion-mesh`.
//! Re-exported here so every existing `bastion::mesh::...` path keeps
//! compiling unchanged.

pub use bastion_mesh::mesh::*;
