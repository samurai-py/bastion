pub mod anthropic;
pub mod gemini;
pub mod groq;
pub mod ollama;
pub mod openai;
pub mod openrouter;
pub mod registry;
pub mod terminal_agent;

use crate::types::{
    CallConfig, ContentPart, LlmResponse, Message, MessageContent, Role, ToolChoice,
};
use async_openai::types::chat::{
    ChatCompletionMessageToolCall, ChatCompletionMessageToolCalls,
    ChatCompletionRequestAssistantMessage, ChatCompletionRequestAssistantMessageContent,
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestSystemMessageContent, ChatCompletionRequestToolMessage,
    ChatCompletionRequestToolMessageContent, ChatCompletionRequestUserMessage,
    ChatCompletionRequestUserMessageContent, ChatCompletionTool, ChatCompletionTools, FunctionCall,
    FunctionObject,
};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Convert Anthropic-format tool defs (`{name, description, input_schema}`,
/// as built by AgentLoop) into async-openai `ChatCompletionTools` for the
/// OpenAI-compatible providers (OpenAI, Gemini, OpenRouter, Ollama).
pub(crate) fn anthropic_tools_to_openai(tools: &[serde_json::Value]) -> Vec<ChatCompletionTools> {
    tools
        .iter()
        .filter_map(|t| {
            let name = t.get("name")?.as_str()?.to_owned();
            let description = t
                .get("description")
                .and_then(|d| d.as_str())
                .map(str::to_owned);
            let parameters = t.get("input_schema").cloned();
            Some(ChatCompletionTools::Function(ChatCompletionTool {
                function: FunctionObject {
                    name,
                    description,
                    parameters,
                    strict: None,
                },
            }))
        })
        .collect()
}

/// Flatten a MessageContent to plain text (joins Text parts; ignores tool parts).
/// `pub(crate)` so `gemini.rs`'s raw-JSON tool-use serializer (`build_gemini_messages`,
/// SO-05) can reuse it instead of duplicating the match arms.
pub(crate) fn content_text(content: &MessageContent) -> String {
    match content {
        MessageContent::Text(t) => t.clone(),
        MessageContent::Parts(parts) => parts
            .iter()
            .filter_map(|p| match p {
                ContentPart::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// Build the full OpenAI-compatible message list (system prompt + conversation)
/// for the OpenAI/Gemini/OpenRouter/Ollama providers. Handles the tool round-trip:
/// assistant `ToolUse` parts → `tool_calls`; `ToolResult` parts → `role:"tool"`
/// messages with `tool_call_id`. Without this, tool-using models never converge.
pub(crate) fn build_openai_messages(
    system_prompt: &str,
    messages: &[Message],
) -> Vec<ChatCompletionRequestMessage> {
    let mut out = Vec::new();

    if !system_prompt.is_empty() {
        out.push(ChatCompletionRequestMessage::System(
            ChatCompletionRequestSystemMessage {
                content: ChatCompletionRequestSystemMessageContent::Text(system_prompt.to_owned()),
                name: None,
            },
        ));
    }

    for msg in messages {
        match msg.role {
            Role::System => {
                out.push(ChatCompletionRequestMessage::System(
                    ChatCompletionRequestSystemMessage {
                        content: ChatCompletionRequestSystemMessageContent::Text(content_text(
                            &msg.content,
                        )),
                        name: None,
                    },
                ));
            }
            Role::User => {
                out.push(ChatCompletionRequestMessage::User(
                    ChatCompletionRequestUserMessage {
                        content: ChatCompletionRequestUserMessageContent::Text(content_text(
                            &msg.content,
                        )),
                        name: None,
                    },
                ));
            }
            Role::Assistant => {
                let mut text = String::new();
                let mut tool_calls = Vec::new();
                if let MessageContent::Parts(parts) = &msg.content {
                    for p in parts {
                        match p {
                            ContentPart::Text { text: t } => {
                                if !text.is_empty() {
                                    text.push('\n');
                                }
                                text.push_str(t);
                            }
                            ContentPart::ToolUse {
                                id, name, input, ..
                            } => {
                                tool_calls.push(ChatCompletionMessageToolCalls::Function(
                                    ChatCompletionMessageToolCall {
                                        id: id.clone(),
                                        function: FunctionCall {
                                            name: name.clone(),
                                            arguments: input.to_string(),
                                        },
                                    },
                                ));
                            }
                            ContentPart::ToolResult { .. } => {}
                        }
                    }
                } else {
                    text = content_text(&msg.content);
                }
                out.push(ChatCompletionRequestMessage::Assistant(
                    ChatCompletionRequestAssistantMessage {
                        // content is optional when tool_calls are present
                        content: if text.is_empty() && !tool_calls.is_empty() {
                            None
                        } else {
                            Some(ChatCompletionRequestAssistantMessageContent::Text(text))
                        },
                        name: None,
                        tool_calls: if tool_calls.is_empty() {
                            None
                        } else {
                            Some(tool_calls)
                        },
                        refusal: None,
                        audio: None,
                        #[allow(deprecated)]
                        function_call: None,
                    },
                ));
            }
            Role::Tool => {
                // Each ToolResult → its own tool message keyed by tool_call_id.
                if let MessageContent::Parts(parts) = &msg.content {
                    for p in parts {
                        if let ContentPart::ToolResult {
                            tool_use_id,
                            content,
                        } = p
                        {
                            out.push(ChatCompletionRequestMessage::Tool(
                                ChatCompletionRequestToolMessage {
                                    content: ChatCompletionRequestToolMessageContent::Text(
                                        content.clone(),
                                    ),
                                    tool_call_id: tool_use_id.clone(),
                                },
                            ));
                        }
                    }
                } else {
                    // Fallback for legacy text-only tool messages (no id available).
                    out.push(ChatCompletionRequestMessage::User(
                        ChatCompletionRequestUserMessage {
                            content: ChatCompletionRequestUserMessageContent::Text(content_text(
                                &msg.content,
                            )),
                            name: None,
                        },
                    ));
                }
            }
        }
    }

    out
}

#[async_trait::async_trait]
pub trait Provider: Send + Sync {
    async fn complete(
        &self,
        messages: &[Message],
        config: &CallConfig,
    ) -> anyhow::Result<LlmResponse>;
    async fn complete_simple(&self, prompt: &str) -> anyhow::Result<String>;
    fn context_limit(&self) -> usize;
    fn model_name(&self) -> &str;
    /// "anthropic" | "openai" | "gemini" | "openrouter" | "ollama"
    fn name(&self) -> &'static str;

    /// D-09 static capability declaration: does this provider's `complete()` honor
    /// `CallConfig.response_format` via a native json_schema-equivalent mechanism
    /// (OpenAI/Groq/OpenRouter's `json_schema`, Ollama's native `format` field)?
    ///
    /// `true` (default) — callers may set `CallConfig.response_format` directly and
    /// trust the provider to enforce it natively.
    /// `false` — callers must route structured-output requests through
    /// `complete_structured_via_forced_tool_call` (Plan 08-03's forced-tool-call
    /// helper), or rely on the provider's own alternate native mechanism handled
    /// internally by its `complete()` impl (e.g. Gemini's `json_object`, the
    /// terminal_agent provider's prompt-injection — see Plan 08-06).
    ///
    /// Consulted by Plan 08-07's caller branching (router/synth/learn) and Plan
    /// 08-03's forced-tool-call helper.
    fn supports_json_schema(&self) -> bool {
        true
    }

    /// Structured completion. Default = prompt-only fallback (complete_simple);
    /// OpenAI-compat overrides to set response_format. Schema is a HINT — the caller
    /// MUST serde-parse-and-retry (AI-SPEC §4b); never assume schema-valid bytes.
    async fn complete_structured(
        &self,
        system: &str,
        user: &str,
        response_schema: serde_json::Value,
        max_tokens: u32,
        temperature: f32,
    ) -> anyhow::Result<String> {
        let _ = (response_schema, max_tokens, temperature);
        self.complete_simple(&format!("{system}\n\n{user}")).await
    }
}

pub type SharedProvider = Arc<RwLock<Box<dyn Provider>>>;

/// Convert an OpenAI-compatible client error into a legible, provider-tagged error.
///
/// `async_openai` fails to parse non-OpenAI error bodies — OpenRouter sends `code`
/// as an integer, Gemini wraps the error in an array — producing opaque messages like
/// `failed to deserialize api response: invalid type: integer 401, expected a string`
/// that bury the real cause. This pulls the API's human-readable `"message"` out of the
/// blob so callers and logs show e.g. `openrouter API error: User not found.` instead.
pub fn clarify_openai_error(provider: &str, err: impl std::fmt::Display) -> anyhow::Error {
    let raw = err.to_string();
    match extract_api_message(&raw) {
        Some(msg) => anyhow::anyhow!("{provider} API error: {msg}"),
        None => anyhow::anyhow!("{provider} API call failed: {raw}"),
    }
}

/// Best-effort extraction of a JSON `"message": "..."` string value (with escape
/// handling) from an arbitrary error blob. Returns None if absent/unparseable.
fn extract_api_message(s: &str) -> Option<String> {
    const KEY: &str = "\"message\"";
    let start = s.find(KEY)? + KEY.len();
    let after_colon = s[start..].find(':')? + start + 1;
    let rest = s[after_colon..].trim_start();
    let mut chars = rest.strip_prefix('"')?.chars();
    let mut out = String::new();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(other) => out.push(other),
                None => break,
            },
            other => out.push(other),
        }
    }
    None
}

/// Exponential backoff retry wrapper for provider calls (D-13: 3 attempts).
/// Does NOT retry on HTTP 400 (context length exceeded — AutoCompact must handle upstream).
pub async fn call_with_retry<F, Fut, T>(mut f: F, max_retries: u32) -> anyhow::Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    let mut delay = tokio::time::Duration::from_millis(500);
    for attempt in 0..=max_retries {
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) if attempt < max_retries => {
                let msg = e.to_string();
                if msg.contains("HTTP 400") {
                    return Err(e);
                }
                tracing::warn!(attempt, delay_ms = delay.as_millis(), error = %e, "LLM call failed, retrying");
                tokio::time::sleep(delay).await;
                delay *= 2;
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}

/// Sentinel capability name used to force a single-tool round-trip for structured
/// output (Plan 08-03). Reserved — never a real MCP tool or user-facing capability.
const STRUCTURED_OUTPUT_TOOL: &str = "__structured_output";

/// D-02 forced-tool-call fallback for providers whose `supports_json_schema()`
/// returns `false` (see `Provider::supports_json_schema`).
///
/// Drives ONE forced-tool-call round-trip through the SAME
/// `CapabilityRegistry::invoke` single-policy-boundary every real tool call
/// already uses — never a parallel dispatch path. Concretely:
/// 1. Register an ephemeral, pure-echo `StructuredOutputCapability` under
///    `STRUCTURED_OUTPUT_TOOL`, scoped to this call via `TurnCapabilityScope`
///    (RAII — cleaned up on `Drop`, even on early `?` return).
/// 2. Call `provider.complete()` with `tool_choice: Forced(STRUCTURED_OUTPUT_TOOL)`
///    and a single tool definition built from `schema` (NOT `response_format` —
///    this helper relies exclusively on `tool_choice`).
/// 3. Extract the model's tool-call arguments and dispatch them through
///    `registry.invoke()` — even though `StructuredOutputCapability::invoke()`
///    is a no-op echo, this hop is mandatory: it is structurally impossible for
///    this helper to bypass the registry and return `tool_call.arguments`
///    directly, which is exactly the guarantee D-02 requires (T-08-03-01).
///
/// The returned JSON string is still a HINT, not schema-validated bytes — callers
/// (Plan 08-07) MUST serde-parse-and-retry per `Provider::complete_structured`'s
/// existing documented contract; this helper does not weaken it (T-08-03-02).
pub async fn complete_structured_via_forced_tool_call(
    provider: &dyn Provider,
    registry: &mut crate::capability::CapabilityRegistry,
    ctx: &crate::capability::InvokeCtx,
    messages: &[Message],
    base_config: &CallConfig,
    schema: serde_json::Value,
) -> anyhow::Result<String> {
    let cap = Arc::new(
        crate::capability::structured_output::StructuredOutputCapability::new(
            STRUCTURED_OUTPUT_TOOL,
            schema.clone(),
        ),
    );
    // RAII — do not manually remove; the scope's `Drop` handles cleanup even on
    // early `?` return below. `_scope` holds the sole `&mut` for its whole
    // lifetime (required so Drop can always clean up), so `registry` is
    // reborrowed immutably through `Deref` below for the `invoke()` call —
    // this cannot register/remove anything, only read/invoke.
    let _scope = crate::capability::TurnCapabilityScope::new(registry, vec![cap]);
    let registry: &crate::capability::CapabilityRegistry = &_scope;

    let tool_def = serde_json::json!({
        "name": STRUCTURED_OUTPUT_TOOL,
        "description": "Emit the structured response matching the required JSON schema",
        "input_schema": schema,
    });

    let forced_config = CallConfig {
        system_prompt: base_config.system_prompt.clone(),
        max_tokens: base_config.max_tokens,
        tools: vec![tool_def],
        response_format: None,
        tool_choice: Some(ToolChoice::Forced(STRUCTURED_OUTPUT_TOOL.to_owned())),
        temperature: base_config.temperature,
    };

    let response = provider.complete(messages, &forced_config).await?;

    let tool_call = response
        .tool_calls
        .and_then(|tc| tc.into_iter().next())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "{}: forced tool_choice returned no tool_calls",
                provider.name()
            )
        })?;

    // Mandatory single-policy-boundary hop (D-02) — never return
    // `tool_call.arguments` directly, even though this capability is a no-op.
    let result = registry
        .invoke(STRUCTURED_OUTPUT_TOOL, tool_call.arguments, ctx)
        .await?;

    Ok(serde_json::to_string(&result)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tool_roundtrip_produces_assistant_tool_calls_and_tool_message() {
        // Simulate one tool round-trip: assistant emits a tool_use, then a tool result.
        let messages = vec![
            Message {
                role: Role::User,
                content: MessageContent::Text("read the file".into()),
            },
            Message {
                role: Role::Assistant,
                content: MessageContent::Parts(vec![
                    ContentPart::Text {
                        text: String::new(),
                    },
                    ContentPart::ToolUse {
                        id: "call_1".into(),
                        name: "read_file".into(),
                        input: json!({"path":"/tmp/x"}),
                        extra: None,
                    },
                ]),
            },
            Message {
                role: Role::Tool,
                content: MessageContent::Parts(vec![ContentPart::ToolResult {
                    tool_use_id: "call_1".into(),
                    content: "hello".into(),
                }]),
            },
        ];

        let out = build_openai_messages("sys prompt", &messages);

        // [System(sys), User, Assistant(tool_calls), Tool(call_1)]
        assert_eq!(out.len(), 4);
        match &out[2] {
            ChatCompletionRequestMessage::Assistant(a) => {
                let tcs = a
                    .tool_calls
                    .as_ref()
                    .expect("assistant must carry tool_calls");
                assert_eq!(tcs.len(), 1);
                match &tcs[0] {
                    ChatCompletionMessageToolCalls::Function(f) => {
                        assert_eq!(f.id, "call_1");
                        assert_eq!(f.function.name, "read_file");
                    }
                    _ => panic!("expected function tool call"),
                }
            }
            _ => panic!("out[2] must be Assistant"),
        }
        match &out[3] {
            ChatCompletionRequestMessage::Tool(t) => assert_eq!(t.tool_call_id, "call_1"),
            _ => panic!("out[3] must be a Tool message with tool_call_id"),
        }
    }

    #[test]
    fn clarify_extracts_real_provider_error_messages() {
        // OpenRouter 401 (code is an integer — what broke async_openai parsing).
        let openrouter = "failed to deserialize api response: error:invalid type: integer `401`, \
                          expected a string content:{\"error\":{\"message\":\"User not found.\",\"code\":401}}";
        let e = clarify_openai_error("openrouter", openrouter);
        assert_eq!(e.to_string(), "openrouter API error: User not found.");

        // Gemini 429 (error wrapped in an array; message before status).
        let gemini = "failed to deserialize api response: missing field `message` content:\
                      [{\"error\":{\"code\":429,\"message\":\"Your prepayment credits are depleted.\",\
                      \"status\":\"RESOURCE_EXHAUSTED\"}}]";
        let e = clarify_openai_error("gemini", gemini);
        assert_eq!(
            e.to_string(),
            "gemini API error: Your prepayment credits are depleted."
        );

        // No parseable message → tagged passthrough (never silently swallow).
        let opaque = "connection reset by peer";
        let e = clarify_openai_error("openai", opaque);
        assert_eq!(
            e.to_string(),
            "openai API call failed: connection reset by peer"
        );
    }

    // -----------------------------------------------------------------
    // complete_structured_via_forced_tool_call
    // -----------------------------------------------------------------

    use crate::capability::{CapabilityRegistry, InvokeCtx};
    use crate::types::{TokenUsage, ToolCall};

    /// Scripted `Provider` mock: returns a forced tool_call when the request's
    /// `tool_choice` is `Forced`, or no tool_calls at all when scripted empty —
    /// mirrors `tests/evals/spy_provider.rs::MockProvider`'s scripting shape.
    struct ForcedToolMock {
        arguments: Option<serde_json::Value>,
    }

    #[async_trait::async_trait]
    impl Provider for ForcedToolMock {
        async fn complete(
            &self,
            _messages: &[Message],
            config: &CallConfig,
        ) -> anyhow::Result<LlmResponse> {
            let Some(ToolChoice::Forced(name)) = &config.tool_choice else {
                anyhow::bail!("ForcedToolMock expects a Forced tool_choice");
            };
            let tool_calls = self.arguments.clone().map(|arguments| {
                vec![ToolCall {
                    id: "1".into(),
                    name: name.clone(),
                    arguments,
                    extra: None,
                }]
            });
            Ok(LlmResponse {
                text: String::new(),
                tool_calls,
                usage: TokenUsage::default(),
            })
        }

        async fn complete_simple(&self, _prompt: &str) -> anyhow::Result<String> {
            unreachable!("not exercised by forced_tool_call tests")
        }

        fn context_limit(&self) -> usize {
            8192
        }

        fn model_name(&self) -> &str {
            "forced-tool-mock"
        }

        fn name(&self) -> &'static str {
            "mock"
        }
    }

    fn test_ctx() -> InvokeCtx {
        // LocalOnly (the strictest tier) — the ephemeral capability's
        // `is_local() == true` must still let this pass through egress.
        InvokeCtx {
            owner: "test-owner".into(),
            privacy_tier: Some(crate::memory::PrivacyTier::LocalOnly),
            needs_approval: false,
        }
    }

    #[tokio::test]
    async fn forced_tool_call_dispatches_through_registry_invoke() {
        let provider = ForcedToolMock {
            arguments: Some(json!({"x": 1})),
        };
        let mut registry = CapabilityRegistry::new();
        let ctx = test_ctx();
        let messages = vec![];
        let config = CallConfig::default();

        let result = complete_structured_via_forced_tool_call(
            &provider,
            &mut registry,
            &ctx,
            &messages,
            &config,
            json!({"type": "object"}),
        )
        .await
        .unwrap();

        assert_eq!(result, r#"{"x":1}"#);
        // The ephemeral capability must be cleaned up (TurnCapabilityScope Drop).
        assert!(registry.is_empty());
    }

    #[tokio::test]
    async fn forced_tool_call_errors_cleanly_when_no_tool_calls_returned() {
        let provider = ForcedToolMock { arguments: None };
        let mut registry = CapabilityRegistry::new();
        let ctx = test_ctx();
        let messages = vec![];
        let config = CallConfig::default();

        let result = complete_structured_via_forced_tool_call(
            &provider,
            &mut registry,
            &ctx,
            &messages,
            &config,
            json!({"type": "object"}),
        )
        .await;

        assert!(result.is_err());
        assert!(registry.is_empty());
    }
}
