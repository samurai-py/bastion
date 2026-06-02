use crate::agent::handle::AgentHandle;

pub mod telegram;
pub mod webhook;

/// Per-channel configuration (CHAN-04).
pub struct ChannelConfig {
    /// Default persona hint forwarded to the router for messages arriving on this channel.
    pub default_persona: Option<String>,
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
