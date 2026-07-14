//! Shim (M2 step 6): `Persona`/`PersonaRegistry`, the router, the runner, and
//! `PersonaResponder` moved to `bastion-personas`. Re-exported here so every
//! existing `bastion::persona::...` path keeps compiling unchanged.

pub use bastion_personas::persona::*;
