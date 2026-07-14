// TEMPORARY re-export shim (M2). Remove by end of M3 (docs/revamp/M1-ADR-substrate-split.md).
//! Unified capability registry — single invoke surface with policy middleware.
//!
//! M2 step 3b: the kernel half of this module (`registry`, `approval`,
//! `structured_output`) moved to `bastion_runtime::capability`. M2 step 5:
//! the MCP→capability adapters (`adapters.rs`) moved to `bastion_mcp` — they
//! are MCP logic (they hold an `Arc<McpClient>`) and register themselves
//! through the registry's public API. Everything is re-exported under the
//! old paths so every existing `crate::capability::...` path keeps
//! compiling unchanged.

pub use bastion_runtime::capability::{approval, registry, structured_output};

pub use bastion_mcp::adapters;
pub use bastion_mcp::adapters::{DirectFnAdapter, McpToolAdapter, NlCommandAdapter};
pub use bastion_runtime::agent::ports::ApprovalGate;
pub use bastion_runtime::capability::{
    ApprovalOutcome, ApprovalRow, ApprovalStatus, Capability, CapabilityRegistry, InvokeCtx,
    NullApprovalGate, SqliteApprovalGate, TaggedValue, TurnCapabilityScope,
};
