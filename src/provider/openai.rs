use async_openai::{
    Client,
    config::OpenAIConfig,
    types::chat::{
        ChatCompletionRequestMessage,
        ChatCompletionRequestSystemMessage,
        ChatCompletionRequestSystemMessageContent,
        ChatCompletionRequestUserMessage,
        ChatCompletionRequestUserMessageContent,
        ChatCompletionRequestAssistantMessage,
        ChatCompletionRequestAssistantMessageContent,
        ChatCompletionMessageToolCalls,
        CreateChatCompletionRequestArgs,
    },
};

use crate::types::{
    CallConfig, LlmResponse, Message, MessageContent, Role, ToolCall, TokenUsage, strip_think,
};
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

    fn messages_to_openai(messages: &[Message]) -> Vec<ChatCompletionRequestMessage> {
        let mut out = Vec::new();
        for msg in messages {
            let text = match &msg.content {
                MessageContent::Text(t) => t.clone(),
                MessageContent::Parts(parts) => parts.iter()
                    .filter_map(|p| match p {
                        crate::types::ContentPart::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            };

            let cm = match msg.role {
                Role::System => ChatCompletionRequestMessage::System(
                    ChatCompletionRequestSystemMessage {
                        content: ChatCompletionRequestSystemMessageContent::Text(text),
                        name: None,
                    }
                ),
                Role::User | Role::Tool => ChatCompletionRequestMessage::User(
                    ChatCompletionRequestUserMessage {
                        content: ChatCompletionRequestUserMessageContent::Text(text),
                        name: None,
                    }
                ),
                Role::Assistant => ChatCompletionRequestMessage::Assistant(
                    ChatCompletionRequestAssistantMessage {
                        content: Some(ChatCompletionRequestAssistantMessageContent::Text(text)),
                        name: None,
                        tool_calls: None,
                        refusal: None,
                        audio: None,
                        #[allow(deprecated)]
                        function_call: None,
                    }
                ),
            };

            out.push(cm);
        }
        out
    }
}

#[async_trait::async_trait]
impl Provider for OpenAIProvider {
    async fn complete(&self, messages: &[Message], config: &CallConfig) -> anyhow::Result<LlmResponse> {
        let mut oai_messages = Vec::new();

        if !config.system_prompt.is_empty() {
            oai_messages.push(ChatCompletionRequestMessage::System(
                ChatCompletionRequestSystemMessage {
                    content: ChatCompletionRequestSystemMessageContent::Text(config.system_prompt.clone()),
                    name: None,
                }
            ));
        }

        oai_messages.extend(Self::messages_to_openai(messages));

        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.model)
            .max_completion_tokens(config.max_tokens)
            .messages(oai_messages)
            .build()?;

        let response = self.client.chat().create(request).await?;

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
            max_tokens: 512,
            ..Default::default()
        };
        let resp = self.complete(&messages, &config).await?;
        Ok(resp.text)
    }

    fn context_limit(&self) -> usize { 128_000 }
    fn model_name(&self) -> &str { &self.model }
    fn name(&self) -> &'static str { "openai" }
}
