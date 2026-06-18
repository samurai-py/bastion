// Webhook channel — POST /webhook, GET /events, POST /mesh/ingest, POST /auth/exchange, POST /mesh/pair.
//
// Security: owner is resolved from a trusted auth-token→owner_id map (CR-03).
// The request body MUST NOT control owner identity. Unknown tokens → 401.
// Errors are mapped to non-2xx status codes without leaking internal detail (CR-05).
use crate::agent::handle::AgentHandle;
use crate::channel::{Channel, OwnerMap};
use crate::mesh::{MeshPeer, MeshPeerMap};
use crate::types::BastionError;
use axum::http::StatusCode;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

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
        let (events_tx, _) = broadcast::channel::<String>(128);
        let mesh_peer_map = Arc::new(RwLock::new(MeshPeerMap::new()));
        // WR-01: fail-closed — refuse to start if APP_JWT_SECRET is not set.
        // Falling back to a hardcoded default is a silent auth bypass once CR-01 is fixed.
        let jwt_secret = std::env::var("APP_JWT_SECRET").map_err(|_| {
            tracing::error!(
                event = "webhook_no_jwt_secret",
                "APP_JWT_SECRET is not set — refusing to start webhook channel (WR-01: silent default is an auth bypass)"
            );
            anyhow::anyhow!("APP_JWT_SECRET must be set; refusing to start with hardcoded default")
        })?;
        serve(agent, &self.addr, self.owner_map, events_tx, mesh_peer_map, jwt_secret).await
    }

    fn default_persona(&self) -> Option<&str> {
        self.default_persona.as_deref()
    }
}

// ─── axum handler ────────────────────────────────────────────────────────────

use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, sse::{Event, Sse, KeepAlive}},
    routing::post,
    Json, Router,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use tokio_stream::wrappers::BroadcastStream;

/// Public type alias for the OTC store shared between the webhook server and skill commands.
///
/// Skill commands insert a code like this:
///   otc_store.write().await.insert(
///       "BAST-XXXX".to_string(),
///       ("device-name".to_string(), std::time::Instant::now()),
///   );
/// The code is consumed by /auth/exchange or /mesh/pair within 5 minutes (CR-02).
pub type OtcStore = Arc<RwLock<std::collections::HashMap<String, (String, std::time::Instant)>>>;

/// Create a new empty OtcStore. Pass to serve_with_mesh so skills can insert codes.
pub fn new_otc_store() -> OtcStore {
    Arc::new(RwLock::new(std::collections::HashMap::new()))
}

/// JWT claims — sub is the device_name / owner identifier issued at /auth/exchange.
/// Used both for signing (auth_exchange_handler) and verification (resolve_owner_or_401).
#[derive(serde::Serialize, serde::Deserialize)]
struct Claims {
    sub: String,
    device: String,
    exp: u64,
}

/// Webhook request body. Owner is NOT accepted here — use the auth-token header (CR-03).
#[derive(Deserialize)]
struct In {
    text: String,
}

#[derive(Serialize)]
struct Out {
    reply: String,
}

/// Shared state threaded through the axum handlers.
#[derive(Clone)]
struct AppState {
    agent: AgentHandle,
    owner_map: Arc<OwnerMap>,
    /// SSE broadcast channel — capacity=128.
    events_tx: broadcast::Sender<String>,
    /// Registry of known mesh peers (owner_id → peer). Populated from bastion.toml at startup.
    mesh_peer_map: Arc<RwLock<MeshPeerMap>>,
    /// OTC store: token → (device_name_or_peer_owner_id, issued_at). 5-min TTL.
    otc_store: Arc<RwLock<std::collections::HashMap<String, (String, std::time::Instant)>>>,
    /// JWT signing secret for /auth/exchange (HS256).
    jwt_secret: String,
    /// Pluggable mesh transport (P2PTransport or relay). None if mesh not configured.
    mesh_transport: Option<crate::mesh::SharedMeshTransport>,
    /// In-memory store of received mesh slices, keyed by from_owner.
    /// Updated by ingest_handler; read by MeshSliceProvider (SEAM #2).
    mesh_slice_store: Option<crate::mesh::context_provider::MeshSliceStore>,
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

/// Resolve owner from x-bastion-token header. Returns None + 401 response on miss.
/// Pattern from CR-03. All protected routes MUST use this.
///
/// Resolution order:
///   1. Try JWT decode (HS256, signed with jwt_secret) → use sub claim as owner_id (CR-01).
///   2. Fall back to static owner_map lookup (pre-existing CLI/API tokens, backward compat).
///   3. Reject with 401 if both fail.
fn resolve_owner_or_401(
    headers: &HeaderMap,
    owner_map: &OwnerMap,
    jwt_secret: &str,
    event_name: &'static str,
) -> Result<String, axum::response::Response> {
    let token = headers
        .get("x-bastion-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    // First try JWT decode (mobile app tokens issued by /auth/exchange). CR-01.
    let validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
    if let Ok(data) = jsonwebtoken::decode::<Claims>(
        token,
        &jsonwebtoken::DecodingKey::from_secret(jwt_secret.as_bytes()),
        &validation,
    ) {
        return Ok(data.claims.sub);
    }

    // Fallback: static owner_map (pre-existing non-JWT tokens — CLI / API keys).
    match owner_map.resolve(token) {
        Some(o) => Ok(o.to_owned()),
        None => {
            tracing::warn!(event = event_name, "unknown or missing x-bastion-token");
            Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({}))).into_response())
        }
    }
}

/// POST /webhook — resolve owner from trusted token header, forward to AgentHandle.
async fn handle(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(p): Json<In>,
) -> impl IntoResponse {
    // CR-03: owner comes from a trusted header map, never from the request body.
    let owner = match resolve_owner_or_401(&headers, &state.owner_map, &state.jwt_secret, "webhook_unauthorized") {
        Ok(o) => o,
        Err(resp) => return resp,
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

/// GET /events — real-time SSE feed.
/// CR-03: same x-bastion-token auth as /webhook.
/// BroadcastStream capacity=128; lagged receivers get Err which is filtered out.
async fn sse_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let _owner = match resolve_owner_or_401(&headers, &state.owner_map, &state.jwt_secret, "sse_unauthorized") {
        Ok(o) => o,
        Err(resp) => return resp,
    };
    let rx = state.events_tx.subscribe();
    let stream = BroadcastStream::new(rx)
        .filter_map(|r| async { r.ok() })
        .map(|msg| Ok::<_, Infallible>(Event::default().data(msg)));
    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(30)))
        .into_response()
}

/// POST /mesh/ingest — receive encrypted MeshEnvelope from a peer daemon.
///
/// Decrypts the envelope via transport.receive() (age E2E decrypt + from_owner verification).
/// On success, stores the slice in mesh_slice_store so MeshSliceProvider can inject it
/// into the system prompt on the next agent turn (SEAM #2).
/// CR-03: auth via x-bastion-token enforced — unauthenticated callers get 401.
async fn ingest_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // CR-03: enforce auth BEFORE body deserialization. Taking the raw body (not
    // Json<...>) prevents Axum's Json extractor from rejecting an unauthenticated
    // request with 415 before resolve_owner_or_401 ever runs (#mesh-ingest-401).
    let _owner = match resolve_owner_or_401(&headers, &state.owner_map, &state.jwt_secret, "mesh_ingest_unauthorized") {
        Ok(o) => o,
        Err(resp) => return resp,
    };
    let envelope: crate::mesh::MeshEnvelope = match serde_json::from_slice(&body) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(event = "mesh_ingest_bad_body", error = %e);
            return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": format!("invalid envelope: {e}") }))).into_response();
        }
    };

    // CR-06: reject envelopes not addressed to this node's owner.
    // local_owner is the value MESH_OWNER_ID (or BASTION_OWNER_ID) was set to at startup.
    // This check is belt-and-suspenders: P2PTransport::receive() also asserts to_owner,
    // but we guard here to return 403 before spending CPU on decryption.
    if let Ok(local_owner) = std::env::var("MESH_OWNER_ID")
        .or_else(|_| std::env::var("BASTION_OWNER_ID"))
    {
        if envelope.to_owner != local_owner {
            tracing::warn!(
                event = "mesh_ingest_wrong_owner",
                to_owner = %envelope.to_owner,
                local_owner = %local_owner,
            );
            return (StatusCode::FORBIDDEN, Json(serde_json::json!({ "error": "envelope addressed to wrong owner" }))).into_response();
        }
    }

    let transport = match &state.mesh_transport {
        Some(t) => t.clone(),
        None => {
            tracing::warn!(event = "mesh_ingest_no_transport", "mesh transport not configured");
            return (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({ "error": "mesh not configured" }))).into_response();
        }
    };
    match transport.receive(envelope).await {
        Ok(slice) => {
            tracing::info!(event = "mesh_ingest_ok", from_owner = %slice.from_owner, count = slice.beliefs.len());
            // Update MeshSliceStore so MeshSliceProvider picks it up on next turn (SEAM #2)
            if let Some(store) = &state.mesh_slice_store {
                let mut s = store.write().await;
                s.insert(slice.from_owner.clone(), slice.beliefs.clone());
            }
            (StatusCode::OK, Json(serde_json::json!({ "status": "accepted", "beliefs": slice.beliefs.len() }))).into_response()
        }
        Err(e) => {
            tracing::warn!(event = "mesh_ingest_error", error = %e);
            (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": e.to_string() }))).into_response()
        }
    }
}

/// POST /auth/exchange { otc: "BAST-XXXX" } → { jwt, device_name }
///
/// Exchange a one-time code (generated by /connect-app command) for a JWT.
/// OTC TTL: 5 minutes. OTC validated against otc_store; deleted on successful exchange.
/// JWT signed with jwt_secret (HS256). No x-bastion-token required — this IS the auth entry point.
async fn auth_exchange_handler(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let otc = match body.get("otc").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "missing otc" }))).into_response(),
    };

    // Validate OTC against store (5-min TTL)
    let result = {
        let store = state.otc_store.read().await;
        store.get(&otc).map(|(device_name, issued_at)| {
            let elapsed = issued_at.elapsed();
            (device_name.clone(), elapsed)
        })
    };

    match result {
        Some((device_name, elapsed)) if elapsed.as_secs() < 300 => {
            // OTC valid — consume it (delete from store)
            state.otc_store.write().await.remove(&otc);

            // Issue JWT (HS256, 90-day expiry).
            // JWT encodes device name in "sub" claim.
            // The issued JWT IS the x-bastion-token used on subsequent requests.
            use jsonwebtoken::{encode, Header, EncodingKey};
            let exp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() + 90 * 24 * 3600; // 90 days
            let claims = Claims { sub: device_name.clone(), device: device_name.clone(), exp };
            match encode(&Header::default(), &claims, &EncodingKey::from_secret(state.jwt_secret.as_bytes())) {
                Ok(jwt) => (StatusCode::OK, Json(serde_json::json!({ "jwt": jwt, "device_name": &device_name }))).into_response(),
                Err(e) => {
                    tracing::error!(event = "auth_exchange_jwt_error", error = %e);
                    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "jwt encoding failed" }))).into_response()
                }
            }
        }
        Some(_) => {
            // OTC expired — consume it anyway to prevent retry.
            // WR-03: return same body as unknown-OTC to prevent enumeration oracle.
            state.otc_store.write().await.remove(&otc);
            tracing::warn!(event = "auth_exchange_expired_otc");
            (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "invalid OTC" }))).into_response()
        }
        None => {
            tracing::warn!(event = "auth_exchange_invalid_otc");
            (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "invalid OTC" }))).into_response()
        }
    }
}

/// SEC-02: returns true for IP addresses that must not be reachable via mesh peer_url.
/// Blocks loopback (127.x, ::1), unspecified (0.0.0.0), RFC1918 private ranges,
/// and IPv6 ULA (fc00::/7).
fn is_private_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            v4.is_loopback()    // 127.0.0.0/8
            || v4.is_private()  // 10.x, 172.16-31.x, 192.168.x
            || v4.is_link_local() // 169.254.x
            || v4.is_unspecified() // 0.0.0.0
            || v4.is_broadcast()   // 255.255.255.255
        }
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback()   // ::1
            || v6.is_unspecified() // ::
            || (v6.segments()[0] & 0xfe00) == 0xfc00 // fc00::/7 ULA
        }
    }
}

/// POST /mesh/pair body.
#[derive(Deserialize)]
struct MeshPairBody {
    token: String,
    peer_url: String,
    age_pubkey: String,
}

/// POST /mesh/pair { token: "BAST-PEER-XXXX", peer_url: "http://...", age_pubkey: "age1..." }
///
/// Validate pairing OTC TTL, register peer in MeshPeerMap, persist to bastion.toml.
/// CR-03: requires x-bastion-token (the pairing initiator must be authenticated).
async fn mesh_pair_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<MeshPairBody>,
) -> impl IntoResponse {
    let _owner = match resolve_owner_or_401(&headers, &state.owner_map, &state.jwt_secret, "mesh_pair_unauthorized") {
        Ok(o) => o,
        Err(resp) => return resp,
    };

    // Validate pairing token (same OTC store used by /connect-app pairing flow)
    let result = {
        let store = state.otc_store.read().await;
        store.get(&body.token).map(|(peer_owner_id, issued_at)| {
            (peer_owner_id.clone(), issued_at.elapsed())
        })
    };

    match result {
        Some((peer_owner_id, elapsed)) if elapsed.as_secs() < 300 => {
            // Token valid — consume it
            state.otc_store.write().await.remove(&body.token);

            // SEC-01: validate age_pubkey format before registering or persisting
            {
                let re = regex::Regex::new(r"^age1[0-9a-z]+$").expect("static regex");
                if !re.is_match(&body.age_pubkey) {
                    tracing::warn!(event = "mesh_pair_invalid_age_pubkey", age_pubkey = %body.age_pubkey);
                    return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "invalid age_pubkey format — must match ^age1[0-9a-z]+$" }))).into_response();
                }
            }

            // SEC-02: validate peer_url before registering — prevent SSRF to loopback/RFC1918/link-local.
            {
                use url::Url;
                let parsed = Url::parse(&body.peer_url).ok()
                    .and_then(|u| if u.scheme() == "https" { Some(u) } else { None });
                let parsed = match parsed {
                    Some(u) => u,
                    None => {
                        tracing::warn!(event = "mesh_pair_invalid_url", url = %body.peer_url);
                        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "peer_url must be a valid https:// URL" }))).into_response();
                    }
                };
                // DNS-resolve and reject private/loopback/link-local addresses
                let host = parsed.host_str().unwrap_or("").to_string();
                match tokio::net::lookup_host(format!("{}:443", host)).await {
                    Ok(addrs) => {
                        for addr in addrs {
                            let ip = addr.ip();
                            if is_private_ip(ip) {
                                tracing::warn!(
                                    event = "mesh_pair_ssrf_blocked",
                                    url = %body.peer_url,
                                    ip = %ip,
                                );
                                return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "peer_url resolves to a private/loopback address" }))).into_response();
                            }
                        }
                    }
                    Err(e) => {
                        // DNS failure — reject (fail-closed; attacker might be testing internal names)
                        tracing::warn!(event = "mesh_pair_dns_failed", url = %body.peer_url, error = %e);
                        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "peer_url DNS resolution failed" }))).into_response();
                    }
                }
            }

            // Register peer in MeshPeerMap
            let peer = MeshPeer {
                peer_url:     body.peer_url.clone(),
                age_pubkey:   body.age_pubkey.clone(),
                allowed_tags: vec![], // set after pairing via config update
            };
            state.mesh_peer_map.write().await.register(peer_owner_id.clone(), peer);

            // Persist to bastion.toml [[mesh.peer]] (best-effort; full persistence in config.rs)
            // allowed_tags starts empty — set post-pairing via config update
            if let Err(e) = crate::config::append_mesh_peer(&peer_owner_id, &body.peer_url, &body.age_pubkey, &[]).await {
                tracing::warn!(event = "mesh_pair_persist_failed", error = %e, "peer registered in memory but toml persist failed");
            }

            tracing::info!(event = "mesh_pair_ok", peer_owner = %peer_owner_id, peer_url = %body.peer_url);
            (StatusCode::OK, Json(serde_json::json!({ "status": "paired", "peer_owner": peer_owner_id }))).into_response()
        }
        Some(_) => {
            // WR-03: return same body as unknown-token to prevent enumeration oracle.
            state.otc_store.write().await.remove(&body.token);
            tracing::warn!(event = "mesh_pair_expired_token");
            (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "invalid pairing token" }))).into_response()
        }
        None => {
            (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "invalid pairing token" }))).into_response()
        }
    }
}

pub async fn serve(
    agent: AgentHandle,
    addr: &str,
    owner_map: OwnerMap,
    events_tx: broadcast::Sender<String>,
    mesh_peer_map: Arc<RwLock<MeshPeerMap>>,
    jwt_secret: String,
) -> anyhow::Result<()> {
    serve_with_mesh(agent, addr, owner_map, events_tx, mesh_peer_map, jwt_secret, None, None, new_otc_store()).await
}

/// Extended serve function that accepts optional mesh transport and slice store.
/// Called by daemon startup when MESH_IDENTITY_KEY is configured.
///
/// `otc_store`: shared OTC store — pass a handle to skill commands so they can insert
/// BAST-XXXX codes for /auth/exchange and /mesh/pair. Use `new_otc_store()` to create one.
pub async fn serve_with_mesh(
    agent: AgentHandle,
    addr: &str,
    owner_map: OwnerMap,
    events_tx: broadcast::Sender<String>,
    mesh_peer_map: Arc<RwLock<MeshPeerMap>>,
    jwt_secret: String,
    mesh_transport: Option<crate::mesh::SharedMeshTransport>,
    mesh_slice_store: Option<crate::mesh::context_provider::MeshSliceStore>,
    otc_store: OtcStore,
) -> anyhow::Result<()> {
    let state = AppState {
        agent,
        owner_map: Arc::new(owner_map),
        events_tx,
        mesh_peer_map,
        otc_store,
        jwt_secret,
        mesh_transport,
        mesh_slice_store,
    };
    let app = Router::new()
        .route("/webhook", post(handle))
        .route("/events", axum::routing::get(sse_handler))
        .route("/mesh/ingest", post(ingest_handler))
        .route("/auth/exchange", post(auth_exchange_handler))
        .route("/mesh/pair", post(mesh_pair_handler))
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
        let (events_tx, _) = broadcast::channel::<String>(128);
        let mesh_peer_map = Arc::new(RwLock::new(MeshPeerMap::new()));
        let otc_store = Arc::new(RwLock::new(std::collections::HashMap::new()));
        let state = AppState {
            agent: h,
            owner_map: Arc::new(map),
            events_tx,
            mesh_peer_map,
            otc_store,
            jwt_secret: "test-secret".to_string(),
            mesh_transport: None,
            mesh_slice_store: None,
        };
        Router::new()
            .route("/webhook", post(handle))
            .route("/events", axum::routing::get(sse_handler))
            .route("/mesh/ingest", post(ingest_handler))
            .route("/auth/exchange", post(auth_exchange_handler))
            .route("/mesh/pair", post(mesh_pair_handler))
            .with_state(state)
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

    /// GET /events without token returns 401.
    #[tokio::test]
    async fn get_events_no_token_returns_401() {
        let app = build_router();
        let req = Request::builder()
            .method("GET")
            .uri("/events")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    /// POST /mesh/ingest with valid token and valid envelope returns 501 (no transport configured).
    #[tokio::test]
    async fn post_mesh_ingest_returns_501_when_no_transport() {
        let app = build_router();
        // Send a valid MeshEnvelope body — transport check happens after JSON parse
        let envelope = serde_json::json!({
            "from_owner": "peer-owner",
            "to_owner": "mario",
            "ciphertext": [],
            "recipient_hint": "age1test"
        });
        let req = Request::builder()
            .method("POST")
            .uri("/mesh/ingest")
            .header("content-type", "application/json")
            .header("x-bastion-token", "token-mario")
            .body(Body::from(envelope.to_string()))
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
    }

    /// POST /mesh/ingest without token returns 401 (not 501).
    #[tokio::test]
    async fn post_mesh_ingest_no_token_returns_401() {
        let app = build_router();
        let req = Request::builder()
            .method("POST")
            .uri("/mesh/ingest")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    /// POST /auth/exchange with missing otc returns 400.
    #[tokio::test]
    async fn post_auth_exchange_missing_otc_returns_400() {
        let app = build_router();
        let req = Request::builder()
            .method("POST")
            .uri("/auth/exchange")
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    /// POST /auth/exchange with invalid otc returns 401.
    #[tokio::test]
    async fn post_auth_exchange_invalid_otc_returns_401() {
        let app = build_router();
        let body = serde_json::json!({ "otc": "BAST-INVALID-OTC" }).to_string();
        let req = Request::builder()
            .method("POST")
            .uri("/auth/exchange")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    /// POST /mesh/pair without token returns 401.
    #[tokio::test]
    async fn post_mesh_pair_no_token_returns_401() {
        let app = build_router();
        let body = serde_json::json!({
            "token": "BAST-PEER-INVALID",
            "peer_url": "http://peer:8080",
            "age_pubkey": "age1test"
        }).to_string();
        let req = Request::builder()
            .method("POST")
            .uri("/mesh/pair")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    // ── CR-02 OtcStore tests ─────────────────────────────────────────────────

    /// CR-02: build_router_with_otc helper — inserts a live OTC into the shared store.
    fn build_router_with_otc(otc: &str, device_name: &str) -> Router {
        let (h, rx) = handle::channel();
        stub_consumer(rx);
        let (events_tx, _) = broadcast::channel::<String>(128);
        let mesh_peer_map = Arc::new(RwLock::new(MeshPeerMap::new()));
        let store = new_otc_store();
        // Pre-insert the OTC so /auth/exchange can consume it
        store.try_write().unwrap().insert(
            otc.to_string(),
            (device_name.to_string(), std::time::Instant::now()),
        );
        let state = AppState {
            agent: h,
            owner_map: Arc::new(OwnerMap::default()),
            events_tx,
            mesh_peer_map,
            otc_store: store,
            jwt_secret: "test-secret".to_string(),
            mesh_transport: None,
            mesh_slice_store: None,
        };
        Router::new()
            .route("/webhook", post(handle))
            .route("/events", axum::routing::get(sse_handler))
            .route("/auth/exchange", post(auth_exchange_handler))
            .route("/mesh/pair", post(mesh_pair_handler))
            .with_state(state)
    }

    /// CR-02: /auth/exchange with a freshly inserted OTC returns 200 + {jwt, device_name}.
    #[tokio::test]
    async fn post_auth_exchange_valid_otc_returns_jwt() {
        let app = build_router_with_otc("BAST-TEST-1234", "mario-phone");
        let body = serde_json::json!({ "otc": "BAST-TEST-1234" }).to_string();
        let req = Request::builder()
            .method("POST")
            .uri("/auth/exchange")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let val: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(val.get("jwt").and_then(|v| v.as_str()).is_some(), "jwt field missing: {val}");
        assert_eq!(val["device_name"], "mario-phone");
    }

    /// CR-02: new_otc_store() is callable and returns a usable Arc<RwLock<HashMap>>.
    #[test]
    fn new_otc_store_is_accessible() {
        let store = new_otc_store();
        store.try_write().unwrap().insert(
            "BAST-XY".to_string(),
            ("dev".to_string(), std::time::Instant::now()),
        );
        assert!(store.try_read().unwrap().contains_key("BAST-XY"));
    }

    // ── CR-01 / WR-01 JWT tests ──────────────────────────────────────────────

    /// Helper: mint a valid HS256 JWT signed with the test secret.
    fn mint_jwt(secret: &str, sub: &str, exp_offset_secs: i64) -> String {
        use jsonwebtoken::{encode, Header, EncodingKey};
        #[derive(serde::Serialize)]
        struct C { sub: String, device: String, exp: u64 }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let exp = if exp_offset_secs >= 0 {
            now + exp_offset_secs as u64
        } else {
            now.saturating_sub((-exp_offset_secs) as u64)
        };
        let claims = C { sub: sub.to_string(), device: sub.to_string(), exp };
        encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes())).unwrap()
    }

    /// CR-01: valid JWT (signed with test-secret, sub="mario-phone") → 200 on /webhook.
    #[tokio::test]
    async fn post_webhook_valid_jwt_returns_200() {
        let app = build_router();
        let jwt = mint_jwt("test-secret", "mario-phone", 3600);
        let body = serde_json::json!({ "text": "hello" }).to_string();
        let req = Request::builder()
            .method("POST")
            .uri("/webhook")
            .header("content-type", "application/json")
            .header("x-bastion-token", jwt)
            .body(Body::from(body))
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    /// CR-01: JWT signed with a different key → 401.
    #[tokio::test]
    async fn post_webhook_jwt_wrong_key_returns_401() {
        let app = build_router();
        let jwt = mint_jwt("wrong-secret", "mario-phone", 3600);
        let body = serde_json::json!({ "text": "hello" }).to_string();
        let req = Request::builder()
            .method("POST")
            .uri("/webhook")
            .header("content-type", "application/json")
            .header("x-bastion-token", jwt)
            .body(Body::from(body))
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    /// CR-01: expired JWT → 401.
    #[tokio::test]
    async fn post_webhook_expired_jwt_returns_401() {
        let app = build_router();
        let jwt = mint_jwt("test-secret", "mario-phone", -3600);
        let body = serde_json::json!({ "text": "hello" }).to_string();
        let req = Request::builder()
            .method("POST")
            .uri("/webhook")
            .header("content-type", "application/json")
            .header("x-bastion-token", jwt)
            .body(Body::from(body))
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    /// CR-01: raw non-JWT string → 401 (backward compat — no match in owner_map either).
    #[tokio::test]
    async fn post_webhook_raw_non_jwt_returns_401() {
        let app = build_router();
        let body = serde_json::json!({ "text": "hello" }).to_string();
        let req = Request::builder()
            .method("POST")
            .uri("/webhook")
            .header("content-type", "application/json")
            .header("x-bastion-token", "not-a-jwt-token-and-not-in-owner-map")
            .body(Body::from(body))
            .unwrap();
        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    /// Backward compat: static owner_map token still works after JWT decode addition.
    #[tokio::test]
    async fn post_webhook_static_owner_map_token_still_works() {
        let app = build_router(); // "token-mario" → "mario" in owner map
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
    }
}
