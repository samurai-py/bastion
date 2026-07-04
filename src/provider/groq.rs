use async_openai::types::chat::{
    CreateChatCompletionRequestArgs, ResponseFormat, ResponseFormatJsonSchema,
};

use super::Provider;
use crate::types::{
    strip_think, CallConfig, LlmResponse, Message, MessageContent, Role, TokenUsage, ToolCall,
};

/// OpenAI-compatible provider pointed at Groq (https://api.groq.com/openai/v1).
/// Groq serves small, fast open models (Llama, Qwen, …) on their LPU stack.
/// Routed when the model name is prefixed `groq/` (e.g. `groq/llama-3.1-8b-instant`,
/// `groq/meta-llama/llama-4-scout-17b-16e-instruct`); the `groq/` prefix is stripped by
/// the registry before this provider is built, so `self.model` is the bare Groq id
/// (which may itself contain a `/`).
///
/// Requests are built with `async-openai`'s request types (they serialize to the OpenAI
/// wire format), but sent and parsed via raw `reqwest`: Groq returns non-OpenAI response
/// fields (e.g. `service_tier: "on_demand"`) that `async-openai`'s strict typed response
/// deserializer rejects. We parse only the fields we need and ignore the rest.
pub struct GroqProvider {
    http: reqwest::Client,
    api_key: String,
    base: String,
    model: String,
}

impl GroqProvider {
    pub fn new(model: &str) -> Self {
        // Groq requires an API key. Reject missing OR empty (avoids an opaque 401).
        let api_key = std::env::var("GROQ_API_KEY").unwrap_or_default();
        if api_key.trim().is_empty() {
            panic!("GROQ_API_KEY required (missing or empty) — get one at https://console.groq.com/keys");
        }
        let base = std::env::var("GROQ_BASE_URL")
            .unwrap_or_else(|_| "https://api.groq.com/openai/v1".to_owned());

        Self {
            http: reqwest::Client::new(),
            api_key,
            base,
            model: model.to_owned(),
        }
    }

    /// POST a chat-completion body and return the parsed JSON, surfacing Groq's error
    /// message on non-2xx. Deliberately lenient: unknown response fields are ignored.
    async fn post_chat(&self, body: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let url = format!("{}/chat/completions", self.base);
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("groq request failed: {e}"))?;

        let status = resp.status();
        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("groq response was not JSON: {e}"))?;

        if !status.is_success() {
            let msg = json
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error");
            anyhow::bail!("groq API error ({}): {msg}", status.as_u16());
        }
        Ok(json)
    }
}

/// Extract the first choice's message content, stripping any `<think>` block.
fn first_content(json: &serde_json::Value) -> &str {
    json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or_default()
}

#[async_trait::async_trait]
impl Provider for GroqProvider {
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
        let body = serde_json::to_value(&request)?;
        let json = self.post_chat(&body).await?;

        let text = strip_think(first_content(&json));

        let tool_calls: Vec<ToolCall> = json["choices"][0]["message"]["tool_calls"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|tc| {
                        let id = tc["id"].as_str()?.to_owned();
                        let name = tc["function"]["name"].as_str()?.to_owned();
                        let arguments = tc["function"]["arguments"]
                            .as_str()
                            .and_then(|s| serde_json::from_str(s).ok())
                            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                        Some(ToolCall {
                            id,
                            name,
                            arguments,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let usage = TokenUsage {
            input_tokens: json["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32,
            output_tokens: json["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32,
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

    /// Structured completion via OpenAI-compatible `response_format: json_schema`.
    /// `strict:false` — Groq's strict validator rejects schemars' representation of
    /// `Option<enum>` (an `anyOf` whose branch is a `$ref`, e.g. the router's
    /// `convene_reason`): "anyOf branches must be disambiguated". Non-strict still
    /// schema-GUIDES generation (verified to return schema-valid JSON) while tolerating
    /// `$ref` branches; the caller serde-parse-retries regardless. OpenRouter/Gemini
    /// accept strict:true, so this leniency is Groq-specific.
    async fn complete_structured(
        &self,
        system: &str,
        user: &str,
        response_schema: serde_json::Value,
        max_tokens: u32,
        temperature: f32,
    ) -> anyhow::Result<String> {
        let user_msg = vec![Message {
            role: Role::User,
            content: MessageContent::Text(user.to_owned()),
        }];
        let oai_messages = super::build_openai_messages(system, &user_msg);

        let mut args = CreateChatCompletionRequestArgs::default();
        args.model(&self.model)
            .max_completion_tokens(max_tokens)
            .temperature(temperature)
            .response_format(ResponseFormat::JsonSchema {
                json_schema: ResponseFormatJsonSchema {
                    name: "structured".into(),
                    description: None,
                    schema: Some(response_schema),
                    strict: Some(false),
                },
            })
            .messages(oai_messages);
        let request = args.build()?;
        let body = serde_json::to_value(&request)?;
        let json = self.post_chat(&body).await?;

        let content = first_content(&json);
        if content.is_empty() {
            anyhow::bail!("Groq structured response had no content");
        }
        Ok(content.to_owned())
    }

    fn context_limit(&self) -> usize {
        131_072
    }
    fn model_name(&self) -> &str {
        &self.model
    }
    fn name(&self) -> &'static str {
        "groq"
    }
}
