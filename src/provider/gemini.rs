use async_openai::{
    Client,
    config::OpenAIConfig,
    types::chat::{ChatCompletionMessageToolCalls, CreateChatCompletionRequestArgs},
};

use crate::types::{CallConfig, LlmResponse, Message, MessageContent, Role, ToolCall, TokenUsage, strip_think};
use super::Provider;

/// OpenAI-compatible provider for Google Gemini via the official compatibility
/// endpoint (https://generativelanguage.googleapis.com/v1beta/openai).
/// Routed when the model name starts with `gemini` (e.g. `gemini-2.0-flash`).
pub struct GeminiProvider {
    client: Client<OpenAIConfig>,
    model:  String,
}

impl GeminiProvider {
    pub fn new(model: &str) -> Self {
        // Gemini requires an API key. Reject missing OR empty (avoids opaque 401).
        let api_key = std::env::var("GEMINI_API_KEY").unwrap_or_default();
        if api_key.trim().is_empty() {
            panic!("GEMINI_API_KEY required (missing or empty) — get one at https://aistudio.google.com/apikey");
        }

        let base = std::env::var("GEMINI_BASE_URL")
            .unwrap_or_else(|_| "https://generativelanguage.googleapis.com/v1beta/openai".to_owned());

        let config = OpenAIConfig::default()
            .with_api_base(base)
            .with_api_key(api_key);

        Self {
            client: Client::with_config(config),
            model:  model.to_owned(),
        }
    }
}

#[async_trait::async_trait]
impl Provider for GeminiProvider {
    async fn complete(&self, messages: &[Message], config: &CallConfig) -> anyhow::Result<LlmResponse> {
        let oai_messages = super::build_openai_messages(&config.system_prompt, messages);

        let mut args = CreateChatCompletionRequestArgs::default();
        args.model(&self.model)
            .max_completion_tokens(config.max_tokens)
            .messages(oai_messages);
        if !config.tools.is_empty() {
            args.tools(super::anthropic_tools_to_openai(&config.tools));
        }
        let request = args.build()?;

        let response = self.client.chat().create(request).await?;

        let choice = response.choices.into_iter().next()
            .ok_or_else(|| anyhow::anyhow!("Gemini returned no choices"))?;

        let raw_text = choice.message.content.unwrap_or_default();
        let text = strip_think(&raw_text);

        let tool_calls: Vec<ToolCall> = choice.message.tool_calls
            .unwrap_or_default()
            .into_iter()
            .filter_map(|tc| match tc {
                ChatCompletionMessageToolCalls::Function(f) => Some(ToolCall {
                    id:        f.id,
                    name:      f.function.name,
                    arguments: serde_json::from_str(&f.function.arguments)
                        .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
                }),
                _ => None,
            })
            .collect();

        let usage = response.usage.map(|u| TokenUsage {
            input_tokens:  u.prompt_tokens,
            output_tokens: u.completion_tokens,
            ..Default::default()
        }).unwrap_or_default();

        Ok(LlmResponse {
            text,
            tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
            usage,
        })
    }

    async fn complete_structured(
        &self,
        system: &str,
        user: &str,
        _response_schema: serde_json::Value,
        max_tokens: u32,
        temperature: f32,
    ) -> anyhow::Result<String> {
        use async_openai::types::chat::ResponseFormat;

        // Gemini's OpenAI-compat endpoint honors response_format=json_object, which forces
        // a clean JSON object (no prose, no ```fences```). The exact field set is described
        // in the system prompt; json_object reliably stops the free-prose/truncation failures
        // the default fallback produced. max_tokens + temperature are respected here, unlike
        // the discard-everything default in the trait.
        let msgs = vec![Message {
            role: Role::User,
            content: MessageContent::Text(user.to_owned()),
        }];
        let oai_messages = super::build_openai_messages(system, &msgs);

        let mut args = CreateChatCompletionRequestArgs::default();
        args.model(&self.model)
            .max_completion_tokens(max_tokens)
            .temperature(temperature)
            .response_format(ResponseFormat::JsonObject)
            .messages(oai_messages);
        let request = args.build()?;

        let response = self.client.chat().create(request).await?;
        let choice = response.choices.into_iter().next()
            .ok_or_else(|| anyhow::anyhow!("Gemini returned no choices"))?;
        let raw = choice.message.content
            .ok_or_else(|| anyhow::anyhow!("Gemini structured response had no content"))?;
        Ok(strip_think(&raw))
    }

    async fn complete_simple(&self, prompt: &str) -> anyhow::Result<String> {
        let messages = vec![Message {
            role:    Role::User,
            content: MessageContent::Text(prompt.to_owned()),
        }];
        let config = CallConfig {
            max_tokens: 512,
            ..Default::default()
        };
        let resp = self.complete(&messages, &config).await?;
        Ok(resp.text)
    }

    fn context_limit(&self) -> usize { 1_000_000 }
    fn model_name(&self) -> &str { &self.model }
    fn name(&self) -> &'static str { "gemini" }
}
