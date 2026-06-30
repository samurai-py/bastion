//! Unified capability registry — single invoke surface with policy middleware.
//!
//! D-13: One canonical capability definition (name + typed I/O + invoke).
//! D-14: SPEC validated with Architect before this implementation.
//!
//! Non-negotiable guardrails (D-13):
//! 1. Uniform interface — registry guarantees, not implementation purity
//! 2. ONE policy middleware at registry boundary (CapabilityRegistry::invoke)
//! 3. No call path bypasses check_egress or approval queue

pub mod adapters;
pub mod registry;

pub use adapters::{DirectFnAdapter, McpToolAdapter, NlCommandAdapter};
pub use registry::{Capability, CapabilityRegistry, InvokeCtx};
