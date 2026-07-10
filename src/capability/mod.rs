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
pub mod approval;
pub mod registry;
pub mod structured_output;

pub use adapters::{DirectFnAdapter, McpToolAdapter, NlCommandAdapter};
pub use approval::{ApprovalOutcome, ApprovalQueue, ApprovalRow, ApprovalStatus};
pub use registry::{Capability, CapabilityRegistry, InvokeCtx, TurnCapabilityScope};
