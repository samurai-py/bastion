use std::time::Duration;
use futures_util::StreamExt;
use serde_json::Value;

use crate::types::{CallConfig, LlmResponse, Message, MessageContent, Role, ToolCall, TokenUsage, strip_think};
use super::Provider;

pub struct AnthropicProvider {
    client:     reqwest::Client,
    api_key:    String,
    model:      String,
    max_tokens: u32,
}

impl AnthropicProvider {
    pub fn new(model: &str) -> Self {
        let api_key = std::env::var("ANTHROPIC_API_KEY")
            .expect("ANTHROPIC_API_KEY required");

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("reqwest client");

        Self {
            client,
            api_key,
            model: model.to_owned(),
            max_tokens: 8192,
        }
    }

    fn messages_to_json(&self, messages: &[Message]) -> Value {
        let mut out = Vec::new();
        for msg in messages {
            let role_str = match msg.role {
                Role::User | Role::Tool | Role::System => "user",
                Role::Assistant => "assistant",
            };
            let content = match &msg.content {
                MessageContent::Text(t) => Value::String(t.clone()),
                MessageContent::Parts(parts) => {
                    let blocks: Vec<Value> = parts.iter().map(|p| {
                        serde_json::to_value(p).unwrap_or(Value::Null)
                    }).collect();
                    Value::Array(blocks)
                }
            };
            out.push(serde_json::json!({ "role": role_str, "content": content }));
        }
        Value::Array(out)
    }
}

#[async_trait::async_trait]
impl Provider for AnthropicProvider {
    async fn complete(&self, messages: &[Message], config: &CallConfig) -> anyhow::Result<LlmResponse> {
        let messages_json = self.messages_to_json(messages);

        let mut body = serde_json::json!({
            "model":      self.model,
            "max_tokens": config.max_tokens,
            "stream":     true,
            "messages":   messages_json,
        });

        if !config.system_prompt.is_empty() {
            body["system"] = Value::String(config.system_prompt.clone());
        }

        if !config.tools.is_empty() {
            body["tools"] = Value::Array(config.tools.clone());
        }

        let resp = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key",         &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type",      "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic HTTP {}: {}", status, &body_text[..body_text.len().min(500)]);
        }

        let mut text = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut usage = TokenUsage::default();

        // SSE streaming state
        let mut current_tool_id    = String::new();
        let mut current_tool_name  = String::new();
        let mut current_tool_input = String::new();
        let mut in_tool_use        = false;

        let mut stream = resp.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let bytes = chunk?;
            let chunk_str = std::str::from_utf8(&bytes)
                .map_err(|e| anyhow::anyhow!("SSE UTF-8 error: {}", e))?;

            for line in chunk_str.lines() {
                let data = match line.strip_prefix("data: ") {
                    Some(d) => d,
                    None    => continue,
                };

                if data == "[DONE]" {
                    break;
                }

                let event: Value = match serde_json::from_str(data) {
                    Ok(v)  => v,
                    Err(e) => {
                        tracing::debug!(error = %e, "SSE parse error — skipping line");
                        continue;
                    }
                };

                let event_type = event["type"].as_str().unwrap_or("");

                match event_type {
                    "content_block_start" => {
                        let block_type = event["content_block"]["type"].as_str().unwrap_or("");
                        if block_type == "tool_use" {
                            in_tool_use        = true;
                            current_tool_id    = event["content_block"]["id"].as_str().unwrap_or("").to_owned();
                            current_tool_name  = event["content_block"]["name"].as_str().unwrap_or("").to_owned();
                            current_tool_input = String::new();
                        }
                    }

                    "content_block_delta" => {
                        let delta_type = event["delta"]["type"].as_str().unwrap_or("");
                        match delta_type {
                            "text_delta" => {
                                if let Some(t) = event["delta"]["text"].as_str() {
                                    print!("{}", t);
                                    text.push_str(t);
                                }
                            }
                            "input_json_delta" => {
                                if let Some(partial) = event["delta"]["partial_json"].as_str() {
                                    current_tool_input.push_str(partial);
                                }
                            }
                            _ => {}
                        }
                    }

                    "content_block_stop" => {
                        if in_tool_use {
                            let arguments: Value = serde_json::from_str(&current_tool_input)
                                .unwrap_or(Value::Object(serde_json::Map::new()));
                            tool_calls.push(ToolCall {
                                id:        std::mem::take(&mut current_tool_id),
                                name:      std::mem::take(&mut current_tool_name),
                                arguments,
                            });
                            current_tool_input = String::new();
                            in_tool_use = false;
                        }
                    }

                    "message_delta" => {
                        if let Some(u) = event["usage"].as_object() {
                            if let Some(out) = u.get("output_tokens").and_then(|v| v.as_u64()) {
                                usage.output_tokens = out as u32;
                            }
                        }
                    }

                    "message_start" => {
                        if let Some(u) = event["message"]["usage"].as_object() {
                            if let Some(inp) = u.get("input_tokens").and_then(|v| v.as_u64()) {
                                usage.input_tokens = inp as u32;
                            }
                        }
                    }

                    "message_stop" => break,

                    _ => {}
                }
            }
        }

        println!();

        let text = strip_think(&text);

        Ok(LlmResponse {
            text,
            tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
            usage,
        })
    }

    async fn complete_simple(&self, prompt: &str) -> anyhow::Result<String> {
        use crate::types::MessageContent;
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

    fn context_limit(&self) -> usize { 200_000 }
    fn model_name(&self) -> &str { &self.model }
    fn name(&self) -> &'static str { "anthropic" }
}
