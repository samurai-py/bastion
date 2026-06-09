use async_openai::{
    Client,
    config::OpenAIConfig,
    types::chat::{
        ChatCompletionMessageToolCalls, ChatCompletionRequestMessage,
        ChatCompletionRequestSystemMessage, ChatCompletionRequestSystemMessageContent,
        ChatCompletionRequestUserMessage, ChatCompletionRequestUserMessageContent,
        CreateChatCompletionRequestArgs, ResponseFormat, ResponseFormatJsonSchema,
    },
};

use crate::types::{CallConfig, LlmResponse, Message, MessageContent, Role, ToolCall, TokenUsage, strip_think};
use super::Provider;

pub struct OpenAIProvider {
    client: Client<OpenAIConfig>,
    model:  String,
}

impl OpenAIProvider {
    pub fn new(model: &str) -> Self {
        // OPENAI_API_KEY is read automatically by OpenAIConfig::default().
        // Panic with a clear message if it's missing.
        if std::env::var("OPENAI_API_KEY").is_err() {
            panic!("OPENAI_API_KEY required");
        }
        Self {
            client: Client::new(),
            model:  model.to_owned(),
        }
    }
}

#[async_trait::async_trait]
impl Provider for OpenAIProvider {
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

        let response = self.client.chat().create(request).await
            .map_err(|e| super::clarify_openai_error(self.name(), e))?;

        let choice = response.choices.into_iter().next()
            .ok_or_else(|| anyhow::anyhow!("OpenAI returned no choices"))?;

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

    async fn complete_simple(&self, prompt: &str) -> anyhow::Result<String> {
        let messages = vec![Message {
            role:    Role::User,
            content: MessageContent::Text(prompt.to_owned()),
        }];
        let config = CallConfig {
            max_tokens: 2048,
            ..Default::default()
        };
        let resp = self.complete(&messages, &config).await?;
        Ok(resp.text)
    }

    fn context_limit(&self) -> usize { 128_000 }
    fn model_name(&self) -> &str { &self.model }
    fn name(&self) -> &'static str { "openai" }

    async fn complete_structured(
        &self,
        system: &str,
        user: &str,
        response_schema: serde_json::Value,
        max_tokens: u32,
        temperature: f32,
    ) -> anyhow::Result<String> {
        let mut messages: Vec<ChatCompletionRequestMessage> = Vec::new();
        if !system.is_empty() {
            messages.push(ChatCompletionRequestMessage::System(
                ChatCompletionRequestSystemMessage {
                    content: ChatCompletionRequestSystemMessageContent::Text(system.to_owned()),
                    name: None,
                },
            ));
        }
        messages.push(ChatCompletionRequestMessage::User(
            ChatCompletionRequestUserMessage {
                content: ChatCompletionRequestUserMessageContent::Text(user.to_owned()),
                name: None,
            },
        ));

        let mut args = CreateChatCompletionRequestArgs::default();
        args.model(&self.model)
            .max_completion_tokens(max_tokens)
            .temperature(temperature)
            .response_format(ResponseFormat::JsonSchema {
                json_schema: ResponseFormatJsonSchema {
                    name: "structured".into(),
                    description: None,
                    schema: Some(response_schema),
                    strict: Some(true),
                },
            })
            .messages(messages);
        let request = args.build()?;

        let response = self.client.chat().create(request).await
            .map_err(|e| super::clarify_openai_error(self.name(), e))?;

        let choice = response.choices.into_iter().next()
            .ok_or_else(|| anyhow::anyhow!("OpenAI returned no choices"))?;

        choice.message.content
            .ok_or_else(|| anyhow::anyhow!("OpenAI structured response had no content"))
    }
}
