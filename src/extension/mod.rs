//! Extension host + package manager (`docs/revamp/C3-extension-protocol-design.md`
//! §3, M4-08..12). Deliberately OUTSIDE the kernel — this is product code
//! (dependency resolution, lockfile, install/upgrade/rollback/revoke), never
//! a `bastion-runtime`/`bastion-extension-protocol` concern.
//!
//! REGRA-MÃE, restated at the ONE place it is actually enforced: installing
//! an extension never grants authority. [`facade::HostFacade`] is the single
//! chokepoint every mechanism (declarative/subprocess/wasm) must go through
//! to register a capability, reach a host, read memory, or bind a socket —
//! mirroring `CapabilityRegistry::invoke`'s "one policy boundary" precedent
//! one layer earlier, at the extension's OWN authority rather than the
//! turn's.
//!
//! Modules:
//! - [`facade`] — `ExtensionInstance` (the mechanism trait) + `HostFacade`
//!   (the enforcement boundary).
//! - [`host`] — `ExtensionHost` (install/upgrade/rollback/revoke, pack
//!   resolution) + `Loadout`.
//! - [`declarative`] — the `Declarative` mechanism (data only, §2).
//! - [`subprocess`] — the `Subprocess` mechanism (separate process,
//!   `env_clear`, versioned stdio protocol, §2).

pub mod declarative;
pub mod facade;
pub mod host;
pub mod subprocess;

pub use facade::{ExtensionInstance, HostFacade};
pub use host::{ExtensionHost, Loadout};
