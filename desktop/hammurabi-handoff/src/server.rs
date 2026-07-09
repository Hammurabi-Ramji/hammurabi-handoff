//! The local Axum backend — one router, one enforcement point.
//!
//! Route map:
//! - `GET  /mesh/feed`      — the RAMesh transcript (structured).
//! - `POST /agent/announce` — Phase 1: the mock agent posts its intent.
//! - `POST /x402/settle`    — Phase 2: mock payment resolution.
//! - `POST /execute`        — Phases 2→4: guarded by the x402 middleware;
//!   on a redeemed receipt the Handshake vault mints (Phase 3) and the
//!   settlement envelope is formatted (Phase 4).
//!
//! The Tauri IPC layer drives this *same* router in-process via
//! `tower::oneshot`, so the x402 guard cannot be bypassed from the UI side.
//! [`serve_loopback`] additionally exposes it on `127.0.0.1` — and nothing
//! else — for headless local agents.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;

use crate::intent::{AgentIntent, INTENT_ANNOUNCEMENT};
use crate::mesh::{MeshBus, MeshEventKind, MeshMessage};
use crate::settlement::{OneShotEnvelope, RELAY_HANDLE};
use crate::vault::{self, CREDENTIAL_MINTED_NOTICE, VAULT_HANDLE};
use crate::x402::{payment_guard, PaymentReceipt, X402Gate, GATE_HANDLE};

/// Shared application state — cheap to clone, all interior-shared.
#[derive(Clone)]
pub struct HandoffState {
    pub mesh: Arc<MeshBus>,
    pub gate: Arc<X402Gate>,
}

impl HandoffState {
    pub fn new() -> Self {
        Self {
            mesh: Arc::new(MeshBus::new()),
            gate: Arc::new(X402Gate::new()),
        }
    }
}

impl Default for HandoffState {
    fn default() -> Self {
        Self::new()
    }
}

/// Build the full router. The x402 guard is layered onto `/execute` only —
/// announcements and the feed are free; execution is metered.
pub fn handoff_router(state: HandoffState) -> Router {
    Router::new()
        .route("/mesh/feed", get(mesh_feed))
        .route("/agent/announce", post(agent_announce))
        .route("/x402/settle", post(x402_settle))
        .route(
            "/execute",
            post(execute).layer(middleware::from_fn_with_state(state.clone(), payment_guard)),
        )
        .with_state(state)
}

/// Serve the router on loopback **only**. Sovereignty invariant: this is
/// the sole bind site in the crate, and it is hardwired to `127.0.0.1`.
pub async fn serve_loopback(state: HandoffState, port: u16) -> std::io::Result<()> {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Hammurabi Handoff listening on http://{addr} (loopback only)");
    axum::serve(listener, handoff_router(state)).await
}

// ── Handlers ──────────────────────────────────────────────────────────

async fn mesh_feed(State(state): State<HandoffState>) -> Json<Vec<MeshMessage>> {
    Json(state.mesh.feed())
}

/// Phase 1 — the mock agent announces its opportunity. An empty body mints
/// the canonical mock intent; a JSON body posts a caller-supplied intent.
async fn agent_announce(
    State(state): State<HandoffState>,
    body: Option<Json<AgentIntent>>,
) -> Json<AgentIntent> {
    let intent = body
        .map(|Json(i)| i)
        .unwrap_or_else(AgentIntent::mock_eth_limit_order);
    state.mesh.post(
        &intent.agent_handle,
        MeshEventKind::Intent,
        INTENT_ANNOUNCEMENT,
    );
    Json(intent)
}

#[derive(Debug, Deserialize)]
struct SettleRequest {
    intent_id: String,
}

/// Phase 2 (resolution) — mock-settle the 0.50 USDC fee for an intent.
async fn x402_settle(
    State(state): State<HandoffState>,
    Json(req): Json<SettleRequest>,
) -> Json<PaymentReceipt> {
    let receipt = state
        .gate
        .mock_settle(&req.intent_id, Utc::now().timestamp());
    state.mesh.post(
        GATE_HANDLE,
        MeshEventKind::Payment,
        &format!(
            "x402 payment of 0.50 USDC settled for intent {} — receipt {}.",
            receipt.intent_id, receipt.receipt_id
        ),
    );
    Json(receipt)
}

/// Phases 3+4 — only reachable through the x402 guard, which injects the
/// burned [`PaymentReceipt`] into request extensions.
async fn execute(
    State(state): State<HandoffState>,
    Extension(receipt): Extension<PaymentReceipt>,
    Json(intent): Json<AgentIntent>,
) -> Response {
    // Defense in depth: the guard already intent-bound the receipt via the
    // header; re-check against the body the vault will actually sign.
    if receipt.intent_id != intent.intent_id {
        return (
            StatusCode::PAYMENT_REQUIRED,
            Json(json!({
                "error": "payment_required",
                "detail": "receipt is bound to a different intent than the request body",
            })),
        )
            .into_response();
    }

    let payload = intent.payload();
    let now = Utc::now().timestamp();

    // Phase 3 — JIT mint (chaos-tested, 60s TTL). The signing key never
    // leaves `vault::mint`'s stack frame.
    let credential = match vault::mint(&payload, &receipt, now) {
        Ok(c) => c,
        Err(e) => {
            state.mesh.post(
                VAULT_HANDLE,
                MeshEventKind::Info,
                &format!("Mint refused: {e}"),
            );
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": "mint_refused", "detail": e.to_string() })),
            )
                .into_response();
        }
    };
    state.mesh.post(
        VAULT_HANDLE,
        MeshEventKind::Credential,
        CREDENTIAL_MINTED_NOTICE,
    );

    // Phase 4 — settlement formatting (stubbed relay; no network I/O).
    let envelope = OneShotEnvelope::package(&payload, &credential, &receipt);
    state.mesh.post(
        RELAY_HANDLE,
        MeshEventKind::Settlement,
        &format!(
            "Settlement envelope {} staged for gasless Stylus broadcast (credential expires at {}).",
            envelope.idempotency_key, credential.expires_at
        ),
    );

    (StatusCode::OK, Json(envelope.to_broadcast_json())).into_response()
}

// ── End-to-end pipeline test ──────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::x402::{INTENT_HEADER, RECEIPT_HEADER, X402_RESTRICTION_NOTICE};
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    async fn send(
        router: &Router,
        method: &str,
        uri: &str,
        headers: &[(&str, &str)],
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        let mut req = Request::builder().method(method).uri(uri);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let req = match body {
            Some(json) => req
                .header("content-type", "application/json")
                .body(Body::from(json.to_string())),
            None => req.body(Body::empty()),
        }
        .expect("request builds");

        let resp = router.clone().oneshot(req).await.expect("infallible");
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
            .await
            .expect("body");
        let value = if bytes.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_slice(&bytes).expect("json body")
        };
        (status, value)
    }

    /// The full Legible Autonomy loop: announce → 402 → settle → execute →
    /// envelope, with the three exact narrative strings in feed order.
    #[tokio::test]
    async fn end_to_end_pipeline_and_transcript() {
        let state = HandoffState::new();
        let router = handoff_router(state.clone());

        // Phase 1 — the agent announces.
        let (status, intent_json) = send(&router, "POST", "/agent/announce", &[], None).await;
        assert_eq!(status, StatusCode::OK);
        let intent: AgentIntent = serde_json::from_value(intent_json.clone()).expect("intent");

        // Phase 2 — unpaid execution is intercepted with 402.
        let (status, refusal) = send(
            &router,
            "POST",
            "/execute",
            &[(INTENT_HEADER, intent.intent_id.as_str())],
            Some(intent_json.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::PAYMENT_REQUIRED);
        assert_eq!(refusal["accepts"][0]["amount_display"], "0.50 USDC");

        // Phase 2 — mock payment resolution.
        let (status, receipt) = send(
            &router,
            "POST",
            "/x402/settle",
            &[],
            Some(serde_json::json!({ "intent_id": intent.intent_id })),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let receipt_id = receipt["receipt_id"].as_str().expect("receipt id");

        // Phases 3+4 — paid execution mints and stages settlement.
        let (status, envelope) = send(
            &router,
            "POST",
            "/execute",
            &[
                (INTENT_HEADER, intent.intent_id.as_str()),
                (RECEIPT_HEADER, receipt_id),
            ],
            Some(intent_json.clone()),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "paid execution succeeds: {envelope}"
        );
        assert_eq!(envelope["method"], "verify_handoff");
        assert_eq!(
            envelope["params"]["issued_at"].as_i64().unwrap() + 60,
            envelope["params"]["expires_at"].as_i64().unwrap(),
            "strict 60s TTL"
        );

        // Replay — the receipt was burned; the gate refuses again.
        let (status, _) = send(
            &router,
            "POST",
            "/execute",
            &[
                (INTENT_HEADER, intent.intent_id.as_str()),
                (RECEIPT_HEADER, receipt_id),
            ],
            Some(intent_json),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::PAYMENT_REQUIRED,
            "burned receipt replays fail"
        );

        // The transcript tells the whole story, in order, verbatim.
        let transcript = state.mesh.transcript();
        let i_announce = transcript.find(INTENT_ANNOUNCEMENT).expect("announcement");
        let i_restrict = transcript
            .find(X402_RESTRICTION_NOTICE)
            .expect("restriction");
        let i_minted = transcript.find(CREDENTIAL_MINTED_NOTICE).expect("minted");
        assert!(
            i_announce < i_restrict && i_restrict < i_minted,
            "narrative order: intent → restriction → credential\n{transcript}"
        );
    }

    /// The guard rejects a receipt spent on a different intent.
    #[tokio::test]
    async fn receipt_cannot_be_spent_cross_intent() {
        let state = HandoffState::new();
        let router = handoff_router(state.clone());

        let (_, intent_a) = send(&router, "POST", "/agent/announce", &[], None).await;
        let (_, intent_b) = send(&router, "POST", "/agent/announce", &[], None).await;
        let a: AgentIntent = serde_json::from_value(intent_a).expect("a");
        let b: AgentIntent = serde_json::from_value(intent_b.clone()).expect("b");

        // Pay for A, try to execute B with A's receipt.
        let (_, receipt) = send(
            &router,
            "POST",
            "/x402/settle",
            &[],
            Some(serde_json::json!({ "intent_id": a.intent_id })),
        )
        .await;
        let (status, _) = send(
            &router,
            "POST",
            "/execute",
            &[
                (INTENT_HEADER, b.intent_id.as_str()),
                (RECEIPT_HEADER, receipt["receipt_id"].as_str().unwrap()),
            ],
            Some(intent_b),
        )
        .await;
        assert_eq!(status, StatusCode::PAYMENT_REQUIRED);
    }
}
