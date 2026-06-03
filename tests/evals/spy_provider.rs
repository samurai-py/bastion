//! SpyProvider: a zero-network Provider implementation used by the eval harness.
//!
//! Records every call into a shared `Vec<String>` so tests can assert that
//! no LocalOnly payload ever resolved to a non-ollama provider name.
//! Also exposes `MockProvider` for scripted structured-completion responses.

use async_trait::async_trait;
use bastion::provider::Provider;
use bastion::types::{CallConfig, LlmResponse, Message, TokenUsage};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// SpyProvider
// ---------------------------------------------------------------------------

/// Records every `complete` / `complete_simple` / `complete_structured` call
/// by pushing `self.name` into `calls`. Never makes a network request.
pub struct SpyProvider {
    /// The provider name this spy impersonates (e.g. "openai", "ollama").
    pub name: &'static str,
    /// Shared call log — each entry is the provider name that was "called".
    pub calls: Arc<Mutex<Vec<String>>>,
}

impl SpyProvider {
    pub fn new(name: &'static str, calls: Arc<Mutex<Vec<String>>>) -> Self {
        Self { name, calls }
    }

    fn record(&self) {
        self.calls.lock().unwrap().push(self.name.to_string());
    }
}

#[async_trait]
impl Provider for SpyProvider {
    async fn complete(&self, _messages: &[Message], _config: &CallConfig) -> anyhow::Result<LlmResponse> {
        self.record();
        Ok(LlmResponse {
            text: "spy-response".into(),
            tool_calls: None,
            usage: TokenUsage::default(),
        })
    }

    async fn complete_simple(&self, _prompt: &str) -> anyhow::Result<String> {
        self.record();
        Ok("spy-simple-response".into())
    }

    async fn complete_structured(
        &self,
        _system: &str,
        _user: &str,
        _response_schema: serde_json::Value,
        _max_tokens: u32,
        _temperature: f32,
    ) -> anyhow::Result<String> {
        self.record();
        Ok(r#"{"spy": true}"#.into())
    }

    fn context_limit(&self) -> usize {
        8192
    }

    fn model_name(&self) -> &str {
        "spy-model"
    }

    fn name(&self) -> &'static str {
        self.name
    }
}

// ---------------------------------------------------------------------------
// MockProvider: scripted structured-completion responses for Cabinet evals
// ---------------------------------------------------------------------------

/// Returns pre-scripted JSON strings for `complete_structured`.
/// When the script has one entry it repeats; otherwise removes and returns front.
pub struct MockProvider {
    pub name: &'static str,
    responses: Mutex<Vec<String>>,
}

impl MockProvider {
    pub fn sequence(name: &'static str, responses: &[&str]) -> Self {
        Self {
            name,
            responses: Mutex::new(responses.iter().map(|s| s.to_string()).collect()),
        }
    }

    pub fn always(name: &'static str, response: &str) -> Self {
        Self::sequence(name, &[response])
    }
}

#[async_trait]
impl Provider for MockProvider {
    async fn complete(&self, _: &[Message], _: &CallConfig) -> anyhow::Result<LlmResponse> {
        Ok(LlmResponse {
            text: "mock-response".into(),
            tool_calls: None,
            usage: TokenUsage::default(),
        })
    }

    async fn complete_simple(&self, _: &str) -> anyhow::Result<String> {
        Ok("mock-simple".into())
    }

    async fn complete_structured(
        &self,
        _system: &str,
        _user: &str,
        _schema: serde_json::Value,
        _max_tokens: u32,
        _temperature: f32,
    ) -> anyhow::Result<String> {
        let mut responses = self.responses.lock().unwrap();
        if responses.len() > 1 {
            Ok(responses.remove(0))
        } else {
            Ok(responses[0].clone())
        }
    }

    fn context_limit(&self) -> usize {
        8192
    }

    fn model_name(&self) -> &str {
        "mock-model"
    }

    fn name(&self) -> &'static str {
        self.name
    }
}
