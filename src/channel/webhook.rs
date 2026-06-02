// Implemented in Task 3.
use crate::agent::handle::AgentHandle;
use crate::channel::Channel;

/// Webhook channel: accepts `POST /webhook` and returns the agent reply as JSON (CHAN-03).
pub struct WebhookChannel {
    pub(crate) addr: String,
    pub(crate) default_persona: Option<String>,
}

impl WebhookChannel {
    pub fn new(addr: impl Into<String>) -> Self {
        Self { addr: addr.into(), default_persona: None }
    }

    pub fn with_default_persona(mut self, persona: impl Into<String>) -> Self {
        self.default_persona = Some(persona.into());
        self
    }
}

#[async_trait::async_trait]
impl Channel for WebhookChannel {
    async fn run(self: Box<Self>, agent: AgentHandle) -> anyhow::Result<()> {
        serve(agent, &self.addr).await
    }

    fn default_persona(&self) -> Option<&str> {
        self.default_persona.as_deref()
    }
}

// ─── axum handler ────────────────────────────────────────────────────────────

use axum::{extract::State, routing::post, Json, Router};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct In {
    text: String,
    owner: Option<String>,
}

#[derive(Serialize)]
struct Out {
    reply: String,
}

/// POST /webhook — serializes through the shared AgentHandle mpsc (Pitfall 6).
async fn handle(State(agent): State<AgentHandle>, Json(p): Json<In>) -> Json<Out> {
    let reply = agent
        .ask(p.text, p.owner.unwrap_or_default())
        .await
        .unwrap_or_default();
    Json(Out { reply })
}

pub async fn serve(agent: AgentHandle, addr: &str) -> anyhow::Result<()> {
    let app = Router::new().route("/webhook", post(handle)).with_state(agent);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

// ─── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::handle;
    use axum::body::Body;
    use http::{Request, StatusCode};
    use tower::ServiceExt;
    use tokio::sync::mpsc;

    fn stub_consumer(mut rx: mpsc::Receiver<crate::agent::handle::AgentRequest>) {
        tokio::spawn(async move {
            while let Some(req) = rx.recv().await {
                let _ = req.reply.send(format!("echo:{}", req.text));
            }
        });
    }

    fn build_router() -> Router {
        let (h, rx) = handle::channel();
        stub_consumer(rx);
        Router::new().route("/webhook", post(handle)).with_state(h)
    }

    #[tokio::test]
    async fn post_webhook_returns_json_reply() {
        let app = build_router();

        let body = serde_json::json!({ "text": "hello", "owner": "mario" }).to_string();
        let req = Request::builder()
            .method("POST")
            .uri("/webhook")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let out: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(out["reply"], "echo:hello");
    }

    #[tokio::test]
    async fn post_webhook_no_owner_uses_empty_string() {
        let app = build_router();

        let body = serde_json::json!({ "text": "ping" }).to_string();
        let req = Request::builder()
            .method("POST")
            .uri("/webhook")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let out: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(out["reply"], "echo:ping");
    }
}
