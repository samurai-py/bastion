use serde_json::Value;
use std::collections::HashMap;

/// A registry entry holds the server label, the full JSON Schema, and the
/// tool description (fed to the LLM for tool selection via list_tool_defs).
struct ToolEntry {
    server_label: String,
    input_schema: Value,
    description: String,
}

pub struct ToolRegistry {
    tool_index: HashMap<String, ToolEntry>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tool_index: HashMap::new(),
        }
    }

    /// Register tools with their full schemas + description (fetched at connect time — CORE-02).
    pub fn register_with_schema(
        &mut self,
        server_label: &str,
        tool_name: String,
        input_schema: Value,
        description: String,
    ) {
        self.tool_index.insert(
            tool_name,
            ToolEntry {
                server_label: server_label.to_owned(),
                input_schema,
                description,
            },
        );
    }

    /// Backward-compat: register without schema (schema defaults to empty object).
    pub fn register(&mut self, server_label: &str, tool_names: Vec<String>) {
        for name in tool_names {
            self.tool_index.insert(
                name,
                ToolEntry {
                    server_label: server_label.to_owned(),
                    input_schema: serde_json::json!({"type": "object", "properties": {}}),
                    description: String::new(),
                },
            );
        }
    }

    /// SORTED by tool name (COST-01/D-14b prerequisite, twin of
    /// `CapabilityRegistry::list_tool_defs`): `self.tool_index` is a `HashMap`, whose
    /// iteration order is unspecified and can shift turn-over-turn even when the
    /// registered tool set is unchanged — an unsorted listing would silently
    /// invalidate Plan 08-10's byte-stable cache-prefix guarantee.
    pub fn list_tool_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.tool_index.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }

    pub fn server_for(&self, tool_name: &str) -> Option<&str> {
        self.tool_index
            .get(tool_name)
            .map(|e| e.server_label.as_str())
    }

    /// Return the full input_schema for a tool (populated at connect_all time).
    pub fn get_tool_schema(&self, tool_name: &str) -> Option<&Value> {
        self.tool_index.get(tool_name).map(|e| &e.input_schema)
    }

    /// Return the tool description (empty string if none was provided).
    pub fn get_tool_description(&self, tool_name: &str) -> Option<&str> {
        self.tool_index
            .get(tool_name)
            .map(|e| e.description.as_str())
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_tool_names_returns_registered_tools_sorted() {
        let mut registry = ToolRegistry::new();
        registry.register_with_schema(
            "server-a",
            "z_tool".into(),
            serde_json::json!({}),
            "z".into(),
        );
        registry.register_with_schema(
            "server-a",
            "a_tool".into(),
            serde_json::json!({}),
            "a".into(),
        );
        registry.register_with_schema(
            "server-a",
            "m_tool".into(),
            serde_json::json!({}),
            "m".into(),
        );

        assert_eq!(
            registry.list_tool_names(),
            vec!["a_tool", "m_tool", "z_tool"]
        );
    }
}
