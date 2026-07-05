use async_openai::{
    config::OpenAIConfig,
    types::chat::{ChatCompletionMessageToolCalls, CreateChatCompletionRequestArgs},
    Client,
};
use serde_json::Value;

use super::Provider;
use crate::types::{
    strip_think, CallConfig, ContentPart, LlmResponse, Message, MessageContent, Role, TokenUsage,
    ToolCall,
};

pub struct OllamaProvider {
    /// Serves the existing, working non-structured path unchanged (`/v1/chat/completions`,
    /// OpenAI-compat shim).
    client: Client<OpenAIConfig>,
    /// Raw client for the native `/api/chat` path (SO-02/D-05) — only exercised when
    /// `CallConfig.response_format` is set. See `complete_native`.
    http: reqwest::Client,
    /// Native API root (`http://localhost:11434`), distinct from the `/v1`-suffixed
    /// OpenAI-compat base the async-openai `client` above already targets.
    base: String,
    model: String,
}

impl OllamaProvider {
    pub fn new(model: &str) -> Self {
        // Ollama does not require auth, but async-openai requires a non-empty key.
        let config = OpenAIConfig::default()
            .with_api_base("http://localhost:11434/v1")
            .with_api_key("ollama");

        Self {
            client: Client::with_config(config),
            http: reqwest::Client::new(),
            base: "http://localhost:11434".to_owned(),
            model: model.to_owned(),
        }
    }

    /// Translate Bastion's `Message` list into Ollama's native `/api/chat` message shape
    /// (`[{"role", "content", ["tool_calls"]}]`) — simpler than the OpenAI-compat wrapper
    /// `super::build_openai_messages` builds (no per-role typed structs, no `tool_call_id`
    /// threading). A minimal hand-rolled translator, per plan: assistant `ToolUse` parts
    /// become native `tool_calls` entries (`arguments` as a JSON object, not a stringified
    /// blob — the native shape's convention, unlike the OpenAI-compat one); `ToolResult`
    /// parts become their own `role:"tool"` message.
    fn native_messages(system_prompt: &str, messages: &[Message]) -> Value {
        let mut out = Vec::new();

        if !system_prompt.is_empty() {
            out.push(serde_json::json!({ "role": "system", "content": system_prompt }));
        }

        for msg in messages {
            match msg.role {
                Role::System => {
                    out.push(serde_json::json!({
                        "role": "system",
                        "content": Self::content_text(&msg.content),
                    }));
                }
                Role::User => {
                    out.push(serde_json::json!({
                        "role": "user",
                        "content": Self::content_text(&msg.content),
                    }));
                }
                Role::Assistant => {
                    let mut text = String::new();
                    let mut tool_calls: Vec<Value> = Vec::new();
                    if let MessageContent::Parts(parts) = &msg.content {
                        for p in parts {
                            match p {
                                ContentPart::Text { text: t } => {
                                    if !text.is_empty() {
                                        text.push('\n');
                                    }
                                    text.push_str(t);
                                }
                                ContentPart::ToolUse { name, input, .. } => {
                                    tool_calls.push(serde_json::json!({
                                        "function": { "name": name, "arguments": input },
                                    }));
                                }
                                ContentPart::ToolResult { .. } => {}
                            }
                        }
                    } else {
                        text = Self::content_text(&msg.content);
                    }
                    let mut entry = serde_json::json!({ "role": "assistant", "content": text });
                    if !tool_calls.is_empty() {
                        entry["tool_calls"] = Value::Array(tool_calls);
                    }
                    out.push(entry);
                }
                Role::Tool => {
                    if let MessageContent::Parts(parts) = &msg.content {
                        for p in parts {
                            if let ContentPart::ToolResult { content, .. } = p {
                                out.push(serde_json::json!({
                                    "role": "tool",
                                    "content": content,
                                }));
                            }
                        }
                    }
                }
            }
        }

        Value::Array(out)
    }

    /// Flatten a `MessageContent` to plain text (mirrors `super::content_text`, kept
    /// private here since that helper isn't `pub(crate)`).
    fn content_text(content: &MessageContent) -> String {
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

    /// Native `/api/chat` request/response path (SO-02/D-05). Used ONLY when
    /// `config.response_format` is set — Ollama's OpenAI-compat `response_format` is a
    /// documented, unresolved upstream bug that silently ignores the schema
    /// (ollama/ollama#10001, Pitfall 1); the native `format` field is the only reliable
    /// way to get GBNF constrained decoding from the llama.cpp backend underneath.
    ///
    /// Field names (`message.content`, `message.tool_calls[].function.{name,arguments}`,
    /// `prompt_eval_count`, `eval_count`) are per Ollama docs/api.md as of the 2026-07
    /// phase-8 research — NOT live-verified (owner has no local model, D-08); confirm
    /// during Phase 12 live validation before trusting in production.
    ///
    /// `#[allow(dead_code)]`: not wired into `complete()` yet — the dispatch branch and
    /// the Pitfall 2 `$ref`/`definitions` diagnostic land in Task 2 of this plan.
    #[allow(dead_code)]
    async fn complete_native(
        &self,
        messages: &[Message],
        config: &CallConfig,
        schema: Value,
    ) -> anyhow::Result<LlmResponse> {
        let mut body = serde_json::json!({
            "model": self.model,
            "messages": Self::native_messages(&config.system_prompt, messages),
            "format": schema,
            "stream": false,
        });

        if !config.tools.is_empty() {
            let native_tools: Vec<Value> = config
                .tools
                .iter()
                .filter_map(|t| {
                    let name = t.get("name")?.as_str()?.to_owned();
                    let description = t
                        .get("description")
                        .and_then(|d| d.as_str())
                        .unwrap_or_default();
                    let parameters = t
                        .get("input_schema")
                        .cloned()
                        .unwrap_or_else(|| serde_json::json!({}));
                    Some(serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": name,
                            "description": description,
                            "parameters": parameters,
                        },
                    }))
                })
                .collect();
            body["tools"] = Value::Array(native_tools);
        }

        let url = format!("{}/api/chat", self.base);
        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("ollama native request failed: {e}"))?;

        let status = resp.status();
        let json: Value = resp
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("ollama native response was not JSON: {e}"))?;

        if !status.is_success() {
            let msg = json
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("unknown error");
            anyhow::bail!("ollama native API error ({}): {msg}", status.as_u16());
        }

        parse_native_response(&json)
    }
}

/// Parse Ollama's native `/api/chat` response shape into `LlmResponse`. Factored out of
/// `complete_native` as a pure fn so it is fixture-testable without a live daemon
/// (D-08 — off-GPU tested).
///
/// The native shape has no `choices[]` wrapper (unlike the OpenAI-compat one) and its
/// `tool_calls[]` entries carry no `id` field — one is synthesized here via an
/// incrementing index (`call_{i}`) since `ToolCall.id` is required downstream.
///
/// T-08-05-03: every field extraction uses `.as_*().unwrap_or(..)` — never
/// `.unwrap()`/`.expect()` on untrusted response data — so a daemon returning a
/// response missing expected fields degrades to zeroed/empty values instead of
/// panicking.
fn parse_native_response(json: &Value) -> anyhow::Result<LlmResponse> {
    let raw_text = json["message"]["content"].as_str().unwrap_or_default();
    let text = strip_think(raw_text);

    let tool_calls: Vec<ToolCall> = json["message"]["tool_calls"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .enumerate()
                .filter_map(|(i, tc)| {
                    let name = tc["function"]["name"].as_str()?.to_owned();
                    let arguments = tc["function"]
                        .get("arguments")
                        .cloned()
                        .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
                    Some(ToolCall {
                        id: format!("call_{i}"),
                        name,
                        arguments,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let usage = TokenUsage {
        input_tokens: json["prompt_eval_count"].as_u64().unwrap_or(0) as u32,
        output_tokens: json["eval_count"].as_u64().unwrap_or(0) as u32,
        ..Default::default()
    };

    Ok(LlmResponse {
        text,
        tool_calls: if tool_calls.is_empty() {
            None
        } else {
            Some(tool_calls)
        },
        usage,
    })
}

#[async_trait::async_trait]
impl Provider for OllamaProvider {
    async fn complete(
        &self,
        messages: &[Message],
        config: &CallConfig,
    ) -> anyhow::Result<LlmResponse> {
        let oai_messages = super::build_openai_messages(&config.system_prompt, messages);

        let mut args = CreateChatCompletionRequestArgs::default();
        args.model(&self.model)
            .max_completion_tokens(config.max_tokens)
            .messages(oai_messages);
        if !config.tools.is_empty() {
            args.tools(super::anthropic_tools_to_openai(&config.tools));
        }
        let request = args.build()?;

        let response = self
            .client
            .chat()
            .create(request)
            .await
            .map_err(|e| super::clarify_openai_error(self.name(), e))?;

        let choice = response
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Ollama returned no choices"))?;

        let raw_text = choice.message.content.unwrap_or_default();
        let text = strip_think(&raw_text);

        let tool_calls: Vec<ToolCall> = choice
            .message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .filter_map(|tc| match tc {
                ChatCompletionMessageToolCalls::Function(f) => Some(ToolCall {
                    id: f.id,
                    name: f.function.name,
                    arguments: serde_json::from_str(&f.function.arguments)
                        .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
                }),
                _ => None,
            })
            .collect();

        let usage = response
            .usage
            .map(|u| TokenUsage {
                input_tokens: u.prompt_tokens,
                output_tokens: u.completion_tokens,
                ..Default::default()
            })
            .unwrap_or_default();

        Ok(LlmResponse {
            text,
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
            usage,
        })
    }

    async fn complete_simple(&self, prompt: &str) -> anyhow::Result<String> {
        let messages = vec![Message {
            role: Role::User,
            content: MessageContent::Text(prompt.to_owned()),
        }];
        let config = CallConfig {
            max_tokens: 2048,
            ..Default::default()
        };
        let resp = self.complete(&messages, &config).await?;
        Ok(resp.text)
    }

    fn context_limit(&self) -> usize {
        8_192
    }
    fn model_name(&self) -> &str {
        &self.model
    }
    fn name(&self) -> &'static str {
        "ollama"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_native_response_plain_text_maps_usage_and_has_no_tool_calls() {
        let json = serde_json::json!({
            "message": { "role": "assistant", "content": "hello world" },
            "done": true,
            "prompt_eval_count": 12,
            "eval_count": 34,
        });

        let resp = parse_native_response(&json).unwrap();

        assert_eq!(resp.text, "hello world");
        assert!(resp.tool_calls.is_none());
        assert_eq!(resp.usage.input_tokens, 12);
        assert_eq!(resp.usage.output_tokens, 34);
    }

    #[test]
    fn parse_native_response_tool_call_synthesizes_incrementing_id() {
        let json = serde_json::json!({
            "message": {
                "role": "assistant",
                "content": "",
                "tool_calls": [
                    { "function": { "name": "get_weather", "arguments": {"city": "SF"} } },
                    { "function": { "name": "get_time", "arguments": {} } },
                ],
            },
            "done": true,
            "prompt_eval_count": 5,
            "eval_count": 7,
        });

        let resp = parse_native_response(&json).unwrap();
        let calls = resp.tool_calls.unwrap();

        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].id, "call_0");
        assert_eq!(calls[0].name, "get_weather");
        assert_eq!(calls[0].arguments, serde_json::json!({"city": "SF"}));
        assert_eq!(calls[1].id, "call_1");
        assert_eq!(calls[1].name, "get_time");
    }

    #[test]
    fn parse_native_response_missing_fields_degrades_to_empty_never_panics() {
        // T-08-05-03: a daemon response missing every expected field must not panic.
        let json = serde_json::json!({});

        let resp = parse_native_response(&json).unwrap();

        assert_eq!(resp.text, "");
        assert!(resp.tool_calls.is_none());
        assert_eq!(resp.usage.input_tokens, 0);
        assert_eq!(resp.usage.output_tokens, 0);
    }

    #[test]
    fn native_messages_translates_system_and_user_and_tool_result() {
        let messages = vec![
            Message {
                role: Role::User,
                content: MessageContent::Text("hi".into()),
            },
            Message {
                role: Role::Tool,
                content: MessageContent::Parts(vec![ContentPart::ToolResult {
                    tool_use_id: "call_0".into(),
                    content: "42".into(),
                }]),
            },
        ];

        let native = OllamaProvider::native_messages("be helpful", &messages);
        let arr = native.as_array().unwrap();

        assert_eq!(arr[0]["role"], "system");
        assert_eq!(arr[0]["content"], "be helpful");
        assert_eq!(arr[1]["role"], "user");
        assert_eq!(arr[1]["content"], "hi");
        assert_eq!(arr[2]["role"], "tool");
        assert_eq!(arr[2]["content"], "42");
    }
}
