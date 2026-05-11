use std::collections::HashMap;
use serde_json::Value;

/// A registry entry holds the server label and the full JSON Schema for the tool.
struct ToolEntry {
    server_label: String,
    input_schema: Value,
}

pub struct ToolRegistry {
    tool_index: HashMap<String, ToolEntry>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tool_index: HashMap::new() }
    }

    /// Register tools with their full schemas (fetched at connect_all time — CORE-02).
    pub fn register_with_schema(&mut self, server_label: &str, tool_name: String, input_schema: Value) {
        self.tool_index.insert(tool_name, ToolEntry {
            server_label: server_label.to_owned(),
            input_schema,
        });
    }

    /// Backward-compat: register without schema (schema defaults to empty object).
    pub fn register(&mut self, server_label: &str, tool_names: Vec<String>) {
        for name in tool_names {
            self.tool_index.insert(name, ToolEntry {
                server_label: server_label.to_owned(),
                input_schema: serde_json::json!({"type": "object", "properties": {}}),
            });
        }
    }

    pub fn list_tool_names(&self) -> Vec<&str> {
        self.tool_index.keys().map(String::as_str).collect()
    }

    pub fn server_for(&self, tool_name: &str) -> Option<&str> {
        self.tool_index.get(tool_name).map(|e| e.server_label.as_str())
    }

    /// Return the full input_schema for a tool (populated at connect_all time).
    pub fn get_tool_schema(&self, tool_name: &str) -> Option<&Value> {
        self.tool_index.get(tool_name).map(|e| &e.input_schema)
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
