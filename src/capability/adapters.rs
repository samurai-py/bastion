use std::sync::Arc;
use serde_json::Value;
use async_trait::async_trait;
use crate::capability::registry::{Capability, InvokeCtx};

/// Wraps an MCP tool dispatch via McpClient.
///
/// McpClient is held as Arc so it can be shared across multiple adapters.
/// Dispatch delegates to call_tool_with_timeout — no business logic in adapter.
pub struct McpToolAdapter {
    pub tool_name: String,
    pub server_label: String,
    pub description: String,
    pub schema: Value,
    /// Shared McpClient reference — injected at registry build time.
    pub mcp: Arc<crate::mcp::McpClient>,
}

#[async_trait]
impl Capability for McpToolAdapter {
    fn name(&self) -> &str { &self.tool_name }
    fn description(&self) -> &str { &self.description }
    fn input_schema(&self) -> &Value { &self.schema }
    async fn invoke(&self, args: Value, _ctx: &InvokeCtx) -> anyhow::Result<Value> {
        // Delegate to McpClient — no business logic here (thin adapter).
        // call_tool_with_timeout looks up server_label via internal ToolRegistry.
        self.mcp.call_tool_with_timeout(&self.tool_name, args).await
    }
}

/// Wraps a Rust function / SKILL.md built-in.
///
/// Used for DirectFn registrations (SkillsLoader stub at this wave; filled by 04-05).
pub struct DirectFnAdapter {
    pub cap_name: String,
    pub cap_description: String,
    pub schema: Value,
    pub func: Arc<dyn Fn(Value) -> anyhow::Result<Value> + Send + Sync>,
}

#[async_trait]
impl Capability for DirectFnAdapter {
    fn name(&self) -> &str { &self.cap_name }
    fn description(&self) -> &str { &self.cap_description }
    fn input_schema(&self) -> &Value { &self.schema }
    async fn invoke(&self, args: Value, _ctx: &InvokeCtx) -> anyhow::Result<Value> {
        (self.func)(args)
    }
}

/// Wraps a command router entry (slash commands: /stop, /model, /as, etc.)
///
/// NAMING CONTRACT: NlCommandAdapter is registered under the key "cmd:{command_name}"
/// (e.g. "cmd:model", "cmd:stop"). The `name()` method returns this prefixed form so
/// that `registry.invoke` can detect it via `cap.name().starts_with("cmd:")` and route
/// it to the "ollama" egress short-circuit (NL commands are local — they never send
/// data to a cloud provider).
///
/// Store `command_name` as "cmd:model" (with prefix), NOT as bare "model".
/// Use `NlCommandAdapter::registry_key(bare)` to build the prefixed form.
pub struct NlCommandAdapter {
    /// Full command name with prefix (e.g. "cmd:model", "cmd:stop", "cmd:as").
    /// MUST start with "cmd:" — this is the egress short-circuit invariant.
    pub command_name: String,
    pub cap_description: String,
    pub schema: Value,
}

impl NlCommandAdapter {
    /// Construct adapter with bare name (e.g. "model") — prefix added automatically.
    pub fn new(bare_name: impl Into<String>, description: impl Into<String>, schema: Value) -> Self {
        let bare: String = bare_name.into();
        Self {
            command_name: Self::registry_key(&bare),
            cap_description: description.into(),
            schema,
        }
    }

    /// The prefixed registry key for a bare command name: "cmd:model", "cmd:stop", etc.
    pub fn registry_key(bare_name: &str) -> String {
        format!("cmd:{}", bare_name)
    }
}

#[async_trait]
impl Capability for NlCommandAdapter {
    /// Returns "cmd:{command_name}" — MUST have "cmd:" prefix for egress short-circuit.
    fn name(&self) -> &str { &self.command_name }
    fn description(&self) -> &str { &self.cap_description }
    fn input_schema(&self) -> &Value { &self.schema }
    async fn invoke(&self, _args: Value, _ctx: &InvokeCtx) -> anyhow::Result<Value> {
        // NL commands are dispatched via existing handle_command in src/agent/command.rs.
        // This adapter is a thin shim for registry routing — actual execution happens in AgentLoop.
        // Return a signal value; AgentLoop detects "cmd:" prefix and dispatches to handle_command.
        Ok(serde_json::json!({"cmd": self.command_name, "routed": true}))
    }
}
