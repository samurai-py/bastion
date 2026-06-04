//! Inference gateway for Python MCP containers (D-08 / D-09).
//!
//! POST /api/infer — receives {prompt, privacy_tier} from skill-writer / self-improving
//! and routes through the existing Provider trait + egress check.
//! Python containers hold ZERO raw API keys.

// RED phase: tests only — implementation pending

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use http::{Request, StatusCode};
    use tower::ServiceExt;

    /// Build a stub router for test use — calls `super::router`.
    /// This will fail to compile until `router` is implemented.
    fn build_router() -> axum::Router {
        use std::sync::Arc;
        use tokio::sync::RwLock;
        // Stub provider that always returns "ok"
        let provider: crate::provider::SharedProvider =
            Arc::new(RwLock::new(Box::new(StubProvider { name: "anthropic", fail: false })
                as Box<dyn crate::provider::Provider>));
        super::router(provider)
    }

    fn build_router_fail() -> axum::Router {
        use std::sync::Arc;
        use tokio::sync::RwLock;
        let provider: crate::provider::SharedProvider =
            Arc::new(RwLock::new(Box::new(StubProvider { name: "anthropic", fail: true })
                as Box<dyn crate::provider::Provider>));
        super::router(provider)
    }

    fn build_router_ollama() -> axum::Router {
        use std::sync::Arc;
        use tokio::sync::RwLock;
        let provider: crate::provider::SharedProvider =
            Arc::new(RwLock::new(Box::new(StubProvider { name: "ollama", fail: false })
                as Box<dyn crate::provider::Provider>));
        super::router(provider)
    }

    struct StubProvider {
        name: &'static str,
        fail: bool,
    }

    #[async_trait::async_trait]
    impl crate::provider::Provider for StubProvider {
        async fn complete(
            &self,
            _messages: &[crate::types::Message],
            _config: &crate::types::CallConfig,
        ) -> anyhow::Result<crate::types::LlmResponse> {
            Ok(crate::types::LlmResponse {
                text: "ok".into(),
                tool_calls: None,
                usage: crate::types::TokenUsage::default(),
            })
        }
        async fn complete_simple(&self, _prompt: &str) -> anyhow::Result<String> {
            if self.fail {
                anyhow::bail!("provider error")
            } else {
                Ok("ok".into())
            }
        }
        fn context_limit(&self) -> usize { 4096 }
        fn model_name(&self) -> &str { self.name }
        fn name(&self) -> &'static str { self.name }
    }

    #[tokio::test]
    async fn infer_invalid_tier_returns_400() {
        let app = build_router();
        let body = serde_json::json!({"prompt": "hi", "privacy_tier": "unknown"}).to_string();
        let req = Request::builder()
            .method("POST")
            .uri("/api/infer")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn infer_local_only_non_ollama_returns_403() {
        let app = build_router(); // provider name = "anthropic"
        let body = serde_json::json!({"prompt": "hi", "privacy_tier": "local_only"}).to_string();
        let req = Request::builder()
            .method("POST")
            .uri("/api/infer")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn infer_cloud_ok_returns_200_with_text() {
        let app = build_router(); // provider name = "anthropic", no fail
        let body = serde_json::json!({"prompt": "hi", "privacy_tier": "cloud_ok"}).to_string();
        let req = Request::builder()
            .method("POST")
            .uri("/api/infer")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let out: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(out["text"], "ok");
    }

    #[tokio::test]
    async fn infer_provider_fail_returns_503() {
        let app = build_router_fail();
        let body = serde_json::json!({"prompt": "hi", "privacy_tier": "cloud_ok"}).to_string();
        let req = Request::builder()
            .method("POST")
            .uri("/api/infer")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn infer_local_only_ollama_returns_200() {
        let app = build_router_ollama();
        let body = serde_json::json!({"prompt": "hi", "privacy_tier": "local_only"}).to_string();
        let req = Request::builder()
            .method("POST")
            .uri("/api/infer")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn infer_error_response_no_internal_detail() {
        let app = build_router_fail();
        let body = serde_json::json!({"prompt": "hi", "privacy_tier": "cloud_ok"}).to_string();
        let req = Request::builder()
            .method("POST")
            .uri("/api/infer")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_ne!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains("provider error"), "must not leak internal error: {text}");
        assert!(!text.contains("panicked"), "must not leak panic: {text}");
    }
}
