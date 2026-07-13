//! Shim (M2 step 5): the MCP client (`client`), Composio OAuth (`oauth`),
//! tool registry (`registry`), `CapabilityRegistry` composition helper
//! (`registry_setup`), and `ToolSource` port impl (`tool_source`) moved to
//! `bastion_mcp`. Re-exported here so every existing `crate::mcp::...` path
//! keeps compiling unchanged.
//!
//! `server` (`BastionMcpServer`) stays local — it depends on
//! `crate::goal`/`crate::persona` (product/cognition layers not part of this
//! extraction step), so it cannot move into `bastion_mcp` without either a
//! cycle back into the app crate or a port-based redesign out of scope here.
//! See `bastion_mcp`'s crate doc for the full rationale.

pub mod server;

pub use bastion_mcp::{client, oauth, registry, registry_setup, tool_source};
pub use bastion_mcp::{ComposioOAuth, McpClient, McpToolSource, ToolRegistry};
pub use server::BastionMcpServer;
