//! Unified capability registry — single invoke surface with policy middleware.
//!
//! M2 step 3b: the kernel half of this module (`registry`, `approval`,
//! `structured_output`) moved to `bastion_runtime::capability`; the MCP→
//! capability adapters (`adapters.rs`) stay here — they are MCP logic (they
//! hold an `Arc<McpClient>`) and register themselves through the registry's
//! public API. Everything is re-exported under the old paths so every
//! existing `crate::capability::...` path keeps compiling unchanged.

pub mod adapters;

pub use bastion_runtime::capability::{approval, registry, structured_output};

pub use adapters::{DirectFnAdapter, McpToolAdapter, NlCommandAdapter};
pub use bastion_runtime::capability::{
    ApprovalOutcome, ApprovalQueue, ApprovalRow, ApprovalStatus, Capability, CapabilityRegistry,
    InvokeCtx, TurnCapabilityScope,
};
