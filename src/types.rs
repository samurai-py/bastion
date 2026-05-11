use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: MessageContent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    Tool,
    System,
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Role::User      => write!(f, "user"),
            Role::Assistant => write!(f, "assistant"),
            Role::Tool      => write!(f, "tool"),
            Role::System    => write!(f, "system"),
        }
    }
}

impl FromStr for Role {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user"      => Ok(Role::User),
            "assistant" => Ok(Role::Assistant),
            "tool"      => Ok(Role::Tool),
            "system"    => Ok(Role::System),
            other       => anyhow::bail!("unknown role: {}", other),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text { text: String },
    ToolUse { id: String, name: String, input: serde_json::Value },
    ToolResult { tool_use_id: String, content: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id:        String,
    pub name:      String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens:  u32,
    pub output_tokens: u32,
    pub cache_read:    u32,
    pub cache_write:   u32,
}

#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub text:       String,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub usage:      TokenUsage,
}

#[derive(Debug, Clone)]
pub struct CallConfig {
    pub system_prompt: String,
    pub max_tokens:    u32,
    pub tools:         Vec<serde_json::Value>,
}

impl Default for CallConfig {
    fn default() -> Self {
        Self {
            system_prompt: String::new(),
            max_tokens:    4096,
            tools:         vec![],
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BastionError {
    #[error("Provider error: {0}")]
    Provider(String),
    #[error("Session error: {0}")]
    Session(String),
    #[error("MCP timeout on tool '{tool}' after {elapsed_ms}ms")]
    McpTimeout { tool: String, elapsed_ms: u64 },
    #[error("Tool loop cap exceeded (10 rounds)")]
    ToolLoopCap,
    #[error("Budget exceeded: daily cap reached")]
    BudgetExceeded,
    #[error("Orphaned tool result — no preceding assistant tool_use")]
    OrphanedToolResult,
}

/// Strip `<think>...</think>` blocks from LLM output (CORE-09).
/// Handles: multiple blocks, multiline content, no blocks (returns clone).
pub fn strip_think(s: &str) -> String {
    let open  = "<think>";
    let close = "</think>";
    let mut result = String::with_capacity(s.len());
    let mut rest   = s;

    loop {
        match rest.find(open) {
            None => {
                result.push_str(rest);
                break;
            }
            Some(start) => {
                result.push_str(&rest[..start]);
                rest = &rest[start + open.len()..];
                match rest.find(close) {
                    None => {
                        // Unclosed <think> — treat the remainder as content to discard
                        break;
                    }
                    Some(end) => {
                        rest = &rest[end + close.len()..];
                    }
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_think_basic() {
        assert_eq!(strip_think("hello <think>reasoning</think> world"), "hello  world");
        assert_eq!(strip_think("no thinks here"), "no thinks here");
        assert_eq!(strip_think("<think>only think</think>"), "");
        assert_eq!(strip_think("a <think>x</think> b <think>y</think> c"), "a  b  c");
        assert_eq!(strip_think("a <think>\nmultiline\n</think> b"), "a  b");
    }

    #[test]
    fn role_roundtrip() {
        assert_eq!("user".parse::<Role>().unwrap(), Role::User);
        assert_eq!("assistant".parse::<Role>().unwrap(), Role::Assistant);
        assert_eq!(Role::Tool.to_string(), "tool");
        assert_eq!("system".parse::<Role>().unwrap(), Role::System);
    }
}
