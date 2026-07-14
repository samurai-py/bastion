//! Shim (M2 step 6): the `.af` interop format (export/import) moved to
//! `bastion-mesh`. Re-exported here so every existing
//! `bastion::interop::...` path keeps compiling unchanged.

pub use bastion_mesh::interop::*;
