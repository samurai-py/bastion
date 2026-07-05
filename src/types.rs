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
            Role::User => write!(f, "user"),
            Role::Assistant => write!(f, "assistant"),
            Role::Tool => write!(f, "tool"),
            Role::System => write!(f, "system"),
        }
    }
}

impl FromStr for Role {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" => Ok(Role::User),
            "assistant" => Ok(Role::Assistant),
            "tool" => Ok(Role::Tool),
            "system" => Ok(Role::System),
            other => anyhow::bail!("unknown role: {}", other),
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
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read: u32,
    pub cache_write: u32,
}

#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub text: String,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub usage: TokenUsage,
}

/// How a provider call should resolve tool selection (D-01/D-09 unification).
///
/// `Forced(String)` carries the target tool/capability name — either a real MCP tool
/// name or the sentinel `"__structured_output"` (Plan 08-03's forced-tool-call helper
/// for providers that don't support `response_format` natively, see
/// `Provider::supports_json_schema`). This is pure request-shaping data: it carries no
/// capability-registry lookup or invocation logic itself (that dispatch lives in the
/// provider `complete()` impls and Plan 08-03).
#[derive(Debug, Clone, PartialEq)]
pub enum ToolChoice {
    /// Provider decides whether/which tool to call (today's implicit default).
    Auto,
    /// Provider must call some tool, but may choose which one.
    Required,
    /// Provider must call the named tool specifically.
    Forced(String),
}

#[derive(Debug, Clone)]
pub struct CallConfig {
    pub system_prompt: String,
    pub max_tokens: u32,
    pub tools: Vec<serde_json::Value>,
    /// JSON-schema payload for a structured-output request. `None` = no structured
    /// output requested. Replaces the schema argument `complete_structured` used to
    /// take positionally (D-01 unification, removed in Plan 08-09).
    pub response_format: Option<serde_json::Value>,
    /// Forces (or requires/leaves auto) tool selection for this call. `None` =
    /// provider default/auto — unchanged behavior from today.
    pub tool_choice: Option<ToolChoice>,
    /// Per-call sampling temperature override. `None` = provider's own hardcoded
    /// default (unchanged from today). `complete_structured`'s removed overrides all
    /// took an explicit `temperature: f32` argument that must not silently vanish
    /// once callers migrate to `CallConfig.temperature` (Plan 08-07).
    pub temperature: Option<f32>,
}

impl Default for CallConfig {
    fn default() -> Self {
        Self {
            system_prompt: String::new(),
            max_tokens: 4096,
            tools: vec![],
            response_format: None,
            tool_choice: None,
            temperature: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
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
    #[error("Privacy egress blocked: local-only context bound for non-Ollama provider")]
    PrivacyEgressBlocked,
    /// Input guardrail rejection — structural input check failed (HOOK-02).
    /// Carries a detail string for logging; MUST NOT be echoed to the client.
    #[error("Input guardrail rejected: {0}")]
    InputGuardrailRejected(String),
}

/// Strip `<think>...</think>` blocks from LLM output (CORE-09).
/// Handles: multiple blocks, multiline content, no blocks (returns clone).
pub fn strip_think(s: &str) -> String {
    let open = "<think>";
    let close = "</think>";
    let mut result = String::with_capacity(s.len());
    let mut rest = s;

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
        assert_eq!(
            strip_think("hello <think>reasoning</think> world"),
            "hello  world"
        );
        assert_eq!(strip_think("no thinks here"), "no thinks here");
        assert_eq!(strip_think("<think>only think</think>"), "");
        assert_eq!(
            strip_think("a <think>x</think> b <think>y</think> c"),
            "a  b  c"
        );
        assert_eq!(strip_think("a <think>\nmultiline\n</think> b"), "a  b");
    }

    #[test]
    fn role_roundtrip() {
        assert_eq!("user".parse::<Role>().unwrap(), Role::User);
        assert_eq!("assistant".parse::<Role>().unwrap(), Role::Assistant);
        assert_eq!(Role::Tool.to_string(), "tool");
        assert_eq!("system".parse::<Role>().unwrap(), Role::System);
    }

    #[test]
    fn call_config_default_has_no_structured_output_request() {
        let cfg = CallConfig::default();
        assert_eq!(cfg.system_prompt, "");
        assert_eq!(cfg.max_tokens, 4096);
        assert!(cfg.tools.is_empty());
        assert!(cfg.response_format.is_none());
        assert!(cfg.tool_choice.is_none());
        assert!(cfg.temperature.is_none());
    }

    #[test]
    fn tool_choice_forced_roundtrips_through_debug_and_clone() {
        let choice = ToolChoice::Forced("__structured_output".into());
        let cloned = choice.clone();
        assert_eq!(choice, cloned);
        assert_eq!(format!("{choice:?}"), "Forced(\"__structured_output\")");
    }
}
