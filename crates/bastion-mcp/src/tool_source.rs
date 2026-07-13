//! [`ToolSource`] port implementation wrapping the concrete [`McpClient`].
//!
//! Both methods are moved verbatim from `agent/loop_.rs` (M2 P3): `tool_defs`
//! is the tool-definition-building block from `run_provider_fallback`, and
//! `call_tool_with_timeout` is a direct passthrough to
//! `McpClient::call_tool_with_timeout` — no logic changes, only the call
//! site's dependency shifts from a concrete `Arc<McpClient>` field to this
//! trait object.

use std::sync::Arc;

use crate::client::McpClient;
use bastion_runtime::agent::ports::ToolSource;

/// The production [`ToolSource`]: sources tool defs and dispatches
/// registry-bypass tool calls straight from the connected MCP servers.
pub struct McpToolSource {
    mcp: Arc<McpClient>,
}

impl McpToolSource {
    /// Wrap an already-connected [`McpClient`] (shared with the
    /// `CapabilityRegistry`'s `McpToolAdapter`s via the same `Arc`).
    pub fn new(mcp: Arc<McpClient>) -> Self {
        Self { mcp }
    }
}

#[async_trait::async_trait]
impl ToolSource for McpToolSource {
    async fn tool_defs(&self) -> anyhow::Result<Vec<serde_json::Value>> {
        // D-12/D-14b: list_tool_names() returns sorted-by-name output (Plan
        // 08-02) — this tools array is part of CallConfig and therefore part
        // of the byte-stable-prefix contract build_system_prompt documents.
        // Moved verbatim from `agent/loop_.rs::run_provider_fallback`.
        let tools = self
            .mcp
            .registry()
            .list_tool_names()
            .iter()
            .map(|name| {
                let schema = self
                    .mcp
                    .registry()
                    .get_tool_schema(name)
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}}));
                serde_json::json!({
                    "name": name,
                    "description": format!("External tool: {}", name),
                    "input_schema": schema
                })
            })
            .collect();
        Ok(tools)
    }

    async fn call_tool_with_timeout(
        &self,
        name: &str,
        args: serde_json::Value,
        owner: &str,
    ) -> anyhow::Result<serde_json::Value> {
        self.mcp.call_tool_with_timeout(name, args, owner).await
    }
}
