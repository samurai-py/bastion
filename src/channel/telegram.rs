// Implemented in Task 2.
use crate::agent::handle::AgentHandle;
use crate::channel::Channel;

/// Telegram long-poll channel (CHAN-02).
pub struct TelegramChannel {
    pub(crate) token: String,
    pub(crate) default_persona: Option<String>,
}

impl TelegramChannel {
    /// Build from the `TELEGRAM_BOT_TOKEN` environment variable.  Errors if not set.
    /// Never logs the token (T-02-23 / Pitfall 7).
    pub fn from_env() -> anyhow::Result<Self> {
        let token = std::env::var("TELEGRAM_BOT_TOKEN")
            .map_err(|_| anyhow::anyhow!("TELEGRAM_BOT_TOKEN not set"))?;
        Ok(Self { token, default_persona: None })
    }

    /// Set the default persona for this channel (CHAN-04).
    pub fn with_default_persona(mut self, persona: impl Into<String>) -> Self {
        self.default_persona = Some(persona.into());
        self
    }
}

#[async_trait::async_trait]
impl Channel for TelegramChannel {
    async fn run(self: Box<Self>, agent: AgentHandle) -> anyhow::Result<()> {
        telegram_loop(&self.token, agent).await
    }

    fn default_persona(&self) -> Option<&str> {
        self.default_persona.as_deref()
    }
}

/// Process a single update (text + chat_id) through the AgentHandle.
/// Factored out for unit testing without a live bot token.
pub async fn handle_update(
    text: String,
    chat_id: String,
    agent: &AgentHandle,
) -> anyhow::Result<String> {
    agent.ask(text, chat_id).await
}

async fn telegram_loop(token: &str, agent: AgentHandle) -> anyhow::Result<()> {
    use frankenstein::client_reqwest::Bot;
    use frankenstein::methods::{GetUpdatesParams, SendMessageParams};
    use frankenstein::updates::UpdateContent;
    use frankenstein::AsyncTelegramApi;

    // Never log the token (T-02-23).
    let bot = Bot::new(token);
    let mut offset: i64 = 0;

    loop {
        let params = GetUpdatesParams::builder()
            .offset(offset)
            .timeout(30_u32)
            .build();

        let updates = match bot.get_updates(&params).await {
            Ok(resp) => resp.result,
            Err(e) => {
                tracing::warn!("Telegram get_updates error: {e}");
                continue;
            }
        };

        for update in updates {
            // Pitfall 2: advance offset FIRST so a malformed update never loops forever.
            offset = i64::from(update.update_id) + 1;

            if let UpdateContent::Message(msg) = &update.content {
                let Some(text) = &msg.text else { continue };
                let chat_id = msg.chat.id.to_string();

                let reply = match handle_update(text.clone(), chat_id.clone(), &agent).await {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!("handle_update error for chat {chat_id}: {e}");
                        continue;
                    }
                };

                let send_params = SendMessageParams::builder()
                    .chat_id(msg.chat.id)
                    .text(reply)
                    .build();

                if let Err(e) = bot.send_message(&send_params).await {
                    tracing::warn!("Telegram send_message error for chat {chat_id}: {e}");
                }
            }
            // Non-message updates: warn and skip (T-02-26, mirror McpClient warn+skip).
            // offset already advanced above.
        }
    }
}

// ─── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::handle;
    use tokio::sync::mpsc;

    /// Stub consumer: replies "echo:{text}".
    fn stub_consumer(mut rx: mpsc::Receiver<crate::agent::handle::AgentRequest>) {
        tokio::spawn(async move {
            while let Some(req) = rx.recv().await {
                let _ = req.reply.send(format!("echo:{}", req.text));
            }
        });
    }

    #[tokio::test]
    async fn handle_update_routes_to_agent() {
        let (h, rx) = handle::channel();
        stub_consumer(rx);

        let reply = handle_update("ping".into(), "42".into(), &h).await.unwrap();
        assert_eq!(reply, "echo:ping");
    }
}
