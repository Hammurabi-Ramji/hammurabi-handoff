//! Tauri IPC commands — the RAMesh frontend's only way to act.
//!
//! Every state-mutating command is driven through the *same Axum router*
//! the loopback HTTP surface uses, via an in-process `tower::oneshot`
//! call. That keeps exactly one x402 enforcement point: the UI cannot
//! reach the vault except through the payment guard, because there is no
//! second code path to reach it by.

#![cfg(feature = "tauri-app")]

use axum::body::Body;
use axum::http::Request;
use axum::Router;
use serde::Serialize;
use serde_json::Value;
use tauri::State;
use tower::ServiceExt;

use crate::mesh::MeshMessage;
use crate::server::HandoffState;
use crate::x402::{INTENT_HEADER, RECEIPT_HEADER};

/// Tauri-managed state: the shared pipeline state plus a clone-cheap router
/// over it for in-process dispatch.
pub struct IpcState {
    pub state: HandoffState,
    pub router: Router,
}

/// What an in-process HTTP round-trip produced. The frontend uses `status`
/// to branch (200 → envelope, 402 → payment terms).
#[derive(Debug, Clone, Serialize)]
pub struct Dispatch {
    pub status: u16,
    pub body: Value,
}

/// Drive one request through the router in-process.
async fn dispatch(
    router: &Router,
    method: &str,
    uri: &str,
    headers: &[(&str, String)],
    body: Option<&Value>,
) -> Result<Dispatch, String> {
    let mut builder = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        builder = builder.header(*name, value);
    }
    let request = match body {
        Some(json) => builder
            .header("content-type", "application/json")
            .body(Body::from(json.to_string())),
        None => builder.body(Body::empty()),
    }
    .map_err(|e| format!("request build failed: {e}"))?;

    let response = router
        .clone()
        .oneshot(request)
        .await
        .map_err(|e| format!("dispatch failed: {e}"))?;
    let status = response.status().as_u16();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .map_err(|e| format!("body read failed: {e}"))?;
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).map_err(|e| format!("non-JSON response: {e}"))?
    };
    Ok(Dispatch { status, body })
}

/// Read the RAMesh feed (read-only — no gate involvement).
#[tauri::command]
pub async fn mesh_feed(ipc: State<'_, IpcState>) -> Result<Vec<MeshMessage>, String> {
    Ok(ipc.state.mesh.feed())
}

/// Phase 1 — post the mock agent intent to the feed.
#[tauri::command]
pub async fn agent_announce(ipc: State<'_, IpcState>) -> Result<Dispatch, String> {
    dispatch(&ipc.router, "POST", "/agent/announce", &[], None).await
}

/// Phase 2 — mock-settle the x402 fee for an intent.
#[tauri::command]
pub async fn x402_settle(ipc: State<'_, IpcState>, intent_id: String) -> Result<Dispatch, String> {
    dispatch(
        &ipc.router,
        "POST",
        "/x402/settle",
        &[],
        Some(&serde_json::json!({ "intent_id": intent_id })),
    )
    .await
}

/// Phases 2→4 — request execution of an intent, optionally presenting a
/// settled receipt. Without a receipt this returns the 402 + terms (and the
/// gate narrates the refusal to the feed); with one it returns the
/// settlement envelope.
#[tauri::command]
pub async fn request_execution(
    ipc: State<'_, IpcState>,
    intent: Value,
    receipt_id: Option<String>,
) -> Result<Dispatch, String> {
    let intent_id = intent["intent_id"].as_str().unwrap_or_default().to_string();
    let mut headers = vec![(INTENT_HEADER, intent_id)];
    if let Some(receipt) = receipt_id {
        headers.push((RECEIPT_HEADER, receipt));
    }
    dispatch(&ipc.router, "POST", "/execute", &headers, Some(&intent)).await
}
