use crate::agent::handle::AgentHandle;
use std::collections::HashMap;

pub mod telegram;
pub mod webhook;

/// Per-channel configuration (CHAN-04).
pub struct ChannelConfig {
    /// Default persona hint forwarded to the router for messages arriving on this channel.
    pub default_persona: Option<String>,
}

/// Trusted owner resolution map — maps an opaque sender credential to a stable owner_id.
///
/// For webhook channels: maps auth-token → owner_id.
/// For Telegram channels: maps chat_id (as string) → owner_id.
///
/// Callers MUST NOT accept owner from request bodies / chat payloads.
/// Any sender whose key is absent from this map is REJECTED (CR-03).
#[derive(Clone, Default)]
pub struct OwnerMap(pub HashMap<String, String>);

impl OwnerMap {
    /// Build from a slice of `(credential, owner_id)` pairs.
    pub fn from_pairs(pairs: &[(&str, &str)]) -> Self {
        OwnerMap(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        )
    }

    /// Resolve a credential to an owner_id. Returns `None` if not in the allowlist.
    pub fn resolve(&self, credential: &str) -> Option<&str> {
        self.0.get(credential).map(String::as_str)
    }
}

/// A `Channel` owns its I/O loop and bridges each inbound message to the single serialized
/// AgentLoop via an [`AgentHandle`] clone.
///
/// Implementing types run their transport loop in [`Channel::run`]; all LLM reasoning stays
/// behind the `AgentLoop`.  Never call a provider directly from a channel.
#[async_trait::async_trait]
pub trait Channel: Send + Sync {
    /// Run the channel's I/O loop forever.  Each inbound message is sent to the AgentLoop;
    /// the reply is returned over the channel's transport.
    async fn run(self: Box<Self>, agent: AgentHandle) -> anyhow::Result<()>;

    /// Optional default persona hint for messages arriving on this channel (CHAN-04).
    fn default_persona(&self) -> Option<&str>;
}
