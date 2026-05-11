use crate::types::Message;

/// Dream extracts durable facts from idle session history.
/// Phase 1: stub only. Activated in Phase 2+ with memupalace integration.
#[async_trait::async_trait]
pub trait Dream: Send + Sync {
    async fn extract_facts(&self, messages: &[Message]) -> anyhow::Result<Vec<String>>;
}

/// No-op implementation for Phase 1.
pub struct NoDream;

#[async_trait::async_trait]
impl Dream for NoDream {
    async fn extract_facts(&self, _messages: &[Message]) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
}
