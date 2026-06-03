// Webhook channel — POST /webhook, JSON reply.
//
// Security: owner is resolved from a trusted auth-token→owner_id map (CR-03).
// The request body MUST NOT control owner identity. Unknown tokens → 401.
// Errors are mapped to non-2xx status codes without leaking internal detail (CR-05).
use crate::agent::handle::AgentHandle;
use crate::channel::{Channel, OwnerMap};
use crate::types::BastionError;
use axum::http::StatusCode;

/// Webhook channel: accepts `POST /webhook` and returns the agent reply as JSON (CHAN-03).
pub struct WebhookChannel {
    pub(crate) addr: String,
    pub(crate) default_persona: Option<String>,
    /// Trusted auth-token → owner_id map. Unknown tokens are rejected with 401.
    pub(crate) owner_map: OwnerMap,
}

impl WebhookChannel {
    pub fn new(addr: impl Into<String>) -> Self {
        Self {
            addr: addr.into(),
            default_persona: None,
            owner_map: OwnerMap::default(),
        }
    }

    pub fn with_default_persona(mut self, persona: impl Into<String>) -> Self {
        self.default_persona = Some(persona.into());
        self
    }

    /// Configure the trusted token→owner map. Without this, all requests are rejected.
    pub fn with_owner_map(mut self, map: OwnerMap) -> Self {
        self.owner_map = map;
        self
    }
}

#[async_trait::async_trait]
impl Channel for WebhookChannel {
    async fn run(self: Box<Self>, agent: AgentHandle) -> anyhow::Result<()> {
        serve(agent, &self.addr, self.owner_map).await
    }

    fn default_persona(&self) -> Option<&str> {
        self.default_persona.as_deref()
    }
}

// ─── axum handler ────────────────────────────────────────────────────────────

use axum::{
    extract::State,
    http::HeaderMap,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Webhook request body. Owner is NOT accepted here — use the auth-token header (CR-03).
#[derive(Deserialize)]
struct In {
    text: String,
}

#[derive(Serialize)]
struct Out {
    reply: String,
}

/// Shared state threaded through the axum handler.
#[derive(Clone)]
struct AppState {
    agent: AgentHandle,
    owner_map: Arc<OwnerMap>,
}

/// Categorize an anyhow error for safe HTTP status mapping.
/// NEVER include the error message in the response body — only log it.
///
/// Matches typed BastionError variants — no string-prefix detection (WR-09).
pub fn error_status(e: &anyhow::Error) -> StatusCode {
    // Walk the error chain looking for BastionError variants
    if let Some(be) = e.downcast_ref::<BastionError>() {
        return match be {
            BastionError::PrivacyEgressBlocked => StatusCode::FORBIDDEN,
            BastionError::BudgetExceeded => StatusCode::TOO_MANY_REQUESTS,
            BastionError::InputGuardrailRejected(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
    }
    StatusCode::INTERNAL_SERVER_ERROR
}

/// POST /webhook — resolve owner from trusted token header, forward to AgentHandle.
async fn handle(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(p): Json<In>,
) -> impl IntoResponse {
    // CR-03: owner comes from a trusted header map, never from the request body.
    let token = headers
        .get("x-bastion-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let owner = match state.owner_map.resolve(token) {
        Some(o) => o.to_owned(),
        None => {
            tracing::warn!(event = "webhook_unauthorized", "unknown or missing x-bastion-token");
            return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({}))).into_response();
        }
    };

    // CR-05: map errors to correct HTTP status; never return 200 on denial.
    match state.agent.ask(p.text, owner).await {
        Ok(reply) => Json(Out { reply }).into_response(),
        Err(e) => {
            let status = error_status(&e);
            tracing::warn!(event = "webhook_turn_error", status = %status, "turn failed");
            // Do not echo internal error detail to the client.
            (status, Json(serde_json::json!({}))).into_response()
        }
    }
}

pub async fn serve(agent: AgentHandle, addr: &str, owner_map: OwnerMap) -> anyhow::Result<()> {
    let state = AppState {
        agent,
        owner_map: Arc::new(owner_map),
    };
    let app = Router::new()
        .route("/webhook", post(handle))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

// ─── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::handle;
    use crate::channel::OwnerMap;
    use axum::body::Body;
    use http::{Request, StatusCode};
    use tower::ServiceExt;
    use tokio::sync::mpsc;

    fn stub_consumer(mut rx: mpsc::Receiver<crate::agent::handle::AgentRequest>) {
        tokio::spawn(async move {
            while let Some(req) = rx.recv().await {
                let _ = req.reply.send(Ok(format!("echo:{}", req.text)));
            }
        });
    }

    fn build_router_with_map(map: OwnerMap) -> Router {
        let (h, rx) = handle::channel();
        stub_consumer(rx);
        let state = AppState {
            agent: h,
            owner_map: Arc::new(map),
        };
        Router::new().route("/webhook", post(handle)).with_state(state)
    }

    fn build_router() -> Router {
        build_router_with_map(OwnerMap::from_pairs(&[("token-mario", "mario")]))
    }

    #[tokio::test]
    async fn post_webhook_valid_token_returns_json_reply() {
        let app = build_router();

        let body = serde_json::json!({ "text": "hello" }).to_string();
        let req = Request::builder()
            .method("POST")
            .uri("/webhook")
            .header("content-type", "application/json")
            .header("x-bastion-token", "token-mario")
            .body(Body::from(body))
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let out: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(out["reply"], "echo:hello");
    }

    #[tokio::test]
    async fn post_webhook_unknown_token_returns_401() {
        let app = build_router();

        let body = serde_json::json!({ "text": "ping" }).to_string();
        let req = Request::builder()
            .method("POST")
            .uri("/webhook")
            .header("content-type", "application/json")
            .header("x-bastion-token", "unknown-token")
            .body(Body::from(body))
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn post_webhook_missing_token_returns_401() {
        let app = build_router();

        let body = serde_json::json!({ "text": "ping" }).to_string();
        let req = Request::builder()
            .method("POST")
            .uri("/webhook")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    /// Verify that error replies have no content leak — body must not contain internal detail.
    #[tokio::test]
    async fn error_response_has_no_content_leak() {
        // Use an empty OwnerMap so ALL requests get 401 — no stub consumer needed.
        let app = build_router_with_map(OwnerMap::default());

        let body = serde_json::json!({ "text": "ping" }).to_string();
        let req = Request::builder()
            .method("POST")
            .uri("/webhook")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_ne!(response.status(), StatusCode::OK, "error must not return 200");

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8_lossy(&bytes);
        // Body must not contain any stack trace, error message, or internal token detail
        assert!(!text.contains("thread"), "stack trace in response: {text}");
        assert!(!text.contains("panicked"), "panic in response: {text}");
    }

    /// Verify error_status maps BastionError variants correctly (WR-09: typed, no string prefix).
    #[test]
    fn error_status_maps_variants() {
        let egress_err = anyhow::anyhow!(BastionError::PrivacyEgressBlocked);
        assert_eq!(error_status(&egress_err), StatusCode::FORBIDDEN);

        let budget_err = anyhow::anyhow!(BastionError::BudgetExceeded);
        assert_eq!(error_status(&budget_err), StatusCode::TOO_MANY_REQUESTS);

        // Guardrail errors are now typed BastionError::InputGuardrailRejected (WR-09)
        let guard_err = anyhow::anyhow!(BastionError::InputGuardrailRejected("input is empty".to_owned()));
        assert_eq!(error_status(&guard_err), StatusCode::BAD_REQUEST);

        // Unknown errors → 500
        let other = anyhow::anyhow!("something exploded");
        assert_eq!(error_status(&other), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
