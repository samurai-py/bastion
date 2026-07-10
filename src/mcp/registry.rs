use serde_json::Value;
use std::collections::HashMap;

/// A registry entry holds the server label, the full JSON Schema, the
/// tool description (fed to the LLM for tool selection via list_tool_defs),
/// and the typed locality flag (Plan 10-08) inherited from the owning
/// server's `McpServerEntry.is_local` config.
struct ToolEntry {
    server_label: String,
    input_schema: Value,
    description: String,
    is_local: bool,
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
    ///
    /// `is_local` (Plan 10-08) is the typed, operator-controlled locality flag from the
    /// owning server's `[mcp.servers.*].is_local` config — NEVER derived from `tool_name`.
    pub fn register_with_schema(
        &mut self,
        server_label: &str,
        tool_name: String,
        input_schema: Value,
        description: String,
        is_local: bool,
    ) {
        self.tool_index.insert(
            tool_name,
            ToolEntry {
                server_label: server_label.to_owned(),
                input_schema,
                description,
                is_local,
            },
        );
    }

    /// Backward-compat: register without schema (schema defaults to empty object).
    pub fn register(&mut self, server_label: &str, tool_names: Vec<String>, is_local: bool) {
        for name in tool_names {
            self.tool_index.insert(
                name,
                ToolEntry {
                    server_label: server_label.to_owned(),
                    input_schema: serde_json::json!({"type": "object", "properties": {}}),
                    description: String::new(),
                    is_local,
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

    /// Whether `tool_name`'s owning MCP server was config-flagged `is_local = true`
    /// (Plan 10-08). Unregistered tool names default to `false` — mirrors
    /// `Capability::is_local()`'s own fail-closed-to-external default.
    pub fn is_local(&self, tool_name: &str) -> bool {
        self.tool_index
            .get(tool_name)
            .map(|e| e.is_local)
            .unwrap_or(false)
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
            false,
        );
        registry.register_with_schema(
            "server-a",
            "a_tool".into(),
            serde_json::json!({}),
            "a".into(),
            false,
        );
        registry.register_with_schema(
            "server-a",
            "m_tool".into(),
            serde_json::json!({}),
            "m".into(),
            false,
        );

        assert_eq!(
            registry.list_tool_names(),
            vec!["a_tool", "m_tool", "z_tool"]
        );
    }

    /// Plan 10-08 Test 1: is_local returns true for a tool registered with is_local: true.
    #[test]
    fn registry_is_local_true_for_locally_flagged_tool() {
        let mut registry = ToolRegistry::new();
        registry.register_with_schema(
            "voice",
            "voice_transcribe".into(),
            serde_json::json!({}),
            "transcribe audio".into(),
            true,
        );
        assert!(registry.is_local("voice_transcribe"));
    }

    /// Plan 10-08 Test 2: is_local returns false for a tool registered with is_local: false
    /// (existing behavior preserved for memupalace/skill-writer/self-improving/content).
    #[test]
    fn registry_is_local_false_for_non_local_tool() {
        let mut registry = ToolRegistry::new();
        registry.register_with_schema(
            "memupalace",
            "memory_embed".into(),
            serde_json::json!({}),
            "embed text".into(),
            false,
        );
        assert!(!registry.is_local("memory_embed"));
    }

    /// Plan 10-08 Test 3: is_local returns false (safe default) for a tool name that
    /// was never registered — mirrors Capability::is_local()'s fail-closed default.
    #[test]
    fn registry_is_local_false_for_unregistered_tool() {
        let registry = ToolRegistry::new();
        assert!(!registry.is_local("never_registered"));
    }
}
