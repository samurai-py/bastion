use tokio::sync::{mpsc, oneshot};

/// A single request sent through the AgentHandle.
///
/// `reply` carries a typed `Result` so real errors propagate back to the caller (WR-10).
/// The channel layer (e.g. `webhook::error_status`) can then classify the error correctly
/// instead of receiving a generic "reply dropped" anyhow string.
pub struct AgentRequest {
    pub text: String,
    pub owner: String,
    pub reply: oneshot::Sender<anyhow::Result<String>>,
}

/// A clonable handle that serializes all inbound messages into ONE AgentLoop task.
///
/// Multiple channels (Telegram, webhook, proactive queue) each hold a clone of this handle.
/// All sends funnel into a single `mpsc::Receiver<AgentRequest>` drained by the AgentLoop,
/// preserving the Phase-1 single-turn invariant.
#[derive(Clone)]
pub struct AgentHandle {
    tx: mpsc::Sender<AgentRequest>,
}

/// Construct a (handle, receiver) pair.  The receiver is given to the AgentLoop task.
pub fn channel() -> (AgentHandle, mpsc::Receiver<AgentRequest>) {
    let (tx, rx) = mpsc::channel(32);
    (AgentHandle { tx }, rx)
}

impl AgentHandle {
    /// Send `text` from `owner` to the serialized AgentLoop and await its reply.
    ///
    /// Returns the typed result from the AgentLoop — callers receive real `BastionError`
    /// variants (e.g. PrivacyEgressBlocked, InputGuardrailRejected) so the channel layer
    /// can map them to correct HTTP/transport status codes (WR-10).
    pub async fn ask(&self, text: String, owner: String) -> anyhow::Result<String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(AgentRequest {
                text,
                owner,
                reply: reply_tx,
            })
            .await
            .map_err(|_| anyhow::anyhow!("AgentLoop receiver dropped"))?;
        // Unwrap the outer oneshot (channel dropped = agent crashed) then the inner Result.
        reply_rx
            .await
            .map_err(|_| anyhow::anyhow!("AgentLoop reply dropped"))?
    }
}

// ─── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::task;

    /// Spawn a stub consumer that drains the receiver sequentially, echoing each message.
    /// Returns a vec that accumulates the received (text, owner) pairs in order.
    fn spawn_stub_consumer(
        mut rx: mpsc::Receiver<AgentRequest>,
    ) -> Arc<Mutex<Vec<(String, String)>>> {
        let log: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let log_clone = log.clone();
        task::spawn(async move {
            while let Some(req) = rx.recv().await {
                log_clone
                    .lock()
                    .unwrap()
                    .push((req.text.clone(), req.owner.clone()));
                let _ = req.reply.send(Ok(format!("echo:{}", req.text)));
            }
        });
        log
    }

    #[tokio::test]
    async fn two_concurrent_clones_both_get_replies() {
        let (handle, rx) = channel();
        let log = spawn_stub_consumer(rx);

        let h1 = handle.clone();
        let h2 = handle.clone();

        // Fire both tasks concurrently.
        let (r1, r2) = tokio::join!(
            async move { h1.ask("hello".into(), "alice".into()).await.unwrap() },
            async move { h2.ask("world".into(), "bob".into()).await.unwrap() },
        );

        assert!(r1.starts_with("echo:"), "r1={r1}");
        assert!(r2.starts_with("echo:"), "r2={r2}");

        // Consumer processed both one-at-a-time (log has exactly 2 entries).
        let entries = log.lock().unwrap();
        assert_eq!(
            entries.len(),
            2,
            "expected 2 processed entries, got {entries:?}"
        );
    }
}
