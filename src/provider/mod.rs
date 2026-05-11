pub mod anthropic;
pub mod openai;
pub mod ollama;
pub mod registry;

use std::sync::Arc;
use tokio::sync::RwLock;
use crate::types::{Message, CallConfig, LlmResponse};

#[async_trait::async_trait]
pub trait Provider: Send + Sync {
    async fn complete(&self, messages: &[Message], config: &CallConfig) -> anyhow::Result<LlmResponse>;
    async fn complete_simple(&self, prompt: &str) -> anyhow::Result<String>;
    fn context_limit(&self) -> usize;
    fn model_name(&self) -> &str;
    /// "anthropic" | "openai" | "ollama"
    fn name(&self) -> &'static str;
}

pub type SharedProvider = Arc<RwLock<Box<dyn Provider>>>;

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
