//! OKX marketplace bridge — the one seam joining the OKX edge (`iedb-oka`) to
//! the Handoff vault.
//!
//! An [`OkxInvocation`] arriving from the OKX.AI storefront is routed to the
//! correct Handoff endpoint and driven through the **same in-process Axum
//! router** (`tower::oneshot`) that the Tauri commands use — so an OKX request
//! passes through the identical x402 gate and vault; there is no second path.
//! The response is wrapped as an [`OkxResult`] (echoing the marketplace
//! `request_id`, mirroring the HTTP status) for the storefront to render.
//!
//! Dependency direction is deliberate: `hammurabi-handoff` depends on
//! `iedb-oka`, never the reverse, so OKX schema churn is contained to this
//! module. The bridge uses `iedb-oka`'s default features only — it drives the
//! local router, never the `okx-live` network client.
//!
//! ## Capability → endpoint map
//! | OKX capability        | Handoff route          | Notes |
//! |-----------------------|------------------------|-------|
//! | `intent_announce`     | `POST /agent/announce` | empty params → mock intent |
//! | `payment_settlement`  | `POST /x402/settle`    | `params.intent_id` |
//! | `credential_minting`  | `POST /execute`        | x402-gated; see param shape below |
//! | `audit_feed`          | `GET  /mesh/feed`      | read-only |
//!
//! ## `credential_minting` param shapes (both accepted)
//! - **Unpaid attempt:** `params` *is* the `AgentIntent` object. No receipt →
//!   the gate returns `402` and [`OkxResult::needs_payment`] is `true`.
//! - **Paid attempt:** `params = { "intent": <AgentIntent>, "receipt_id": "…" }`.
//!   The receipt id is forwarded as the `x-402-receipt` header.

use axum::body::Body;
use axum::http::Request;
use axum::Router;
use iedb_oka::interop::{OkxInvocation, OkxResult};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::x402::{INTENT_HEADER, RECEIPT_HEADER};

/// Route one OKX invocation through the Handoff router and wrap the response.
///
/// Returns `Err` only for an unknown capability or a transport-level failure
/// building/reading the in-process request — a `402`/`422` from the gate/vault
/// is a normal `Ok(OkxResult)` with that status, not an error.
pub async fn dispatch_okx(router: &Router, inv: &OkxInvocation) -> Result<OkxResult, String> {
    let method: &str;
    let uri: &str;
    let mut headers: Vec<(&str, String)> = Vec::new();
    let mut body: Option<Value> = None;

    match inv.capability.as_str() {
        "audit_feed" => {
            method = "GET";
            uri = "/mesh/feed";
        }
        "intent_announce" => {
            method = "POST";
            uri = "/agent/announce";
            body = announce_body(&inv.params);
        }
        "payment_settlement" => {
            method = "POST";
            uri = "/x402/settle";
            let intent_id = inv
                .params
                .get("intent_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            body = Some(json!({ "intent_id": intent_id }));
        }
        "credential_minting" => {
            method = "POST";
            uri = "/execute";
            let (intent, receipt) = split_minting_params(&inv.params);
            let intent_id = intent
                .get("intent_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            headers.push((INTENT_HEADER, intent_id));
            if let Some(receipt_id) = receipt {
                headers.push((RECEIPT_HEADER, receipt_id));
            }
            body = Some(intent);
        }
        other => return Err(format!("unknown OKX capability: {other}")),
    }

    let (status, resp_body) = oneshot_json(router, method, uri, &headers, body.as_ref()).await?;
    Ok(OkxResult::for_invocation(inv, status, resp_body))
}

/// Announce with empty/null params mints the server-side mock intent; a
/// non-empty object is forwarded as a caller-supplied intent.
fn announce_body(params: &Value) -> Option<Value> {
    match params {
        Value::Null => None,
        Value::Object(map) if map.is_empty() => None,
        other => Some(other.clone()),
    }
}

/// Split `credential_minting` params into (intent, optional receipt id),
/// accepting both the bare-intent and the `{intent, receipt_id}` shapes.
fn split_minting_params(params: &Value) -> (Value, Option<String>) {
    match params.get("intent") {
        Some(intent) => {
            let receipt = params
                .get("receipt_id")
                .and_then(Value::as_str)
                .map(str::to_owned);
            (intent.clone(), receipt)
        }
        None => (params.clone(), None),
    }
}

/// Drive one request through the router in-process and decode the JSON body.
async fn oneshot_json(
    router: &Router,
    method: &str,
    uri: &str,
    headers: &[(&str, String)],
    body: Option<&Value>,
) -> Result<(u16, Value), String> {
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
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).map_err(|e| format!("non-JSON response: {e}"))?
    };
    Ok((status, value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::{handoff_router, HandoffState};

    fn inv(capability: &str, request_id: &str, params: Value) -> OkxInvocation {
        OkxInvocation {
            capability: capability.to_string(),
            request_id: request_id.to_string(),
            params,
        }
    }

    /// The full OKX marketplace loop, driven entirely through OkxInvocations:
    /// announce → unpaid mint (402) → settle → paid mint (200 envelope) →
    /// audit feed. Proves the OKX edge reaches the vault through the gate.
    #[tokio::test]
    async fn okx_invocation_drives_full_vault_loop() {
        let router = handoff_router(HandoffState::new());

        // 1 — announce (empty params → server mints the mock intent)
        let r = dispatch_okx(&router, &inv("intent_announce", "okx-1", Value::Null))
            .await
            .expect("announce dispatch");
        assert_eq!(r.status, 200);
        assert_eq!(r.request_id, "okx-1", "correlation id echoed");
        let intent = r.body.clone();
        let intent_id = intent["intent_id"].as_str().expect("intent id").to_string();

        // 2 — credential_minting with no receipt → 402, storefront must prompt payment
        let r = dispatch_okx(&router, &inv("credential_minting", "okx-2", intent.clone()))
            .await
            .expect("unpaid mint dispatch");
        assert_eq!(r.status, 402);
        assert!(r.needs_payment(), "storefront sees payment-required");

        // 3 — settle the x402 fee
        let r = dispatch_okx(
            &router,
            &inv(
                "payment_settlement",
                "okx-3",
                json!({ "intent_id": intent_id }),
            ),
        )
        .await
        .expect("settle dispatch");
        assert_eq!(r.status, 200);
        let receipt_id = r.body["receipt_id"]
            .as_str()
            .expect("receipt id")
            .to_string();

        // 4 — credential_minting with the receipt → 200 settlement envelope
        let r = dispatch_okx(
            &router,
            &inv(
                "credential_minting",
                "okx-4",
                json!({ "intent": intent, "receipt_id": receipt_id }),
            ),
        )
        .await
        .expect("paid mint dispatch");
        assert_eq!(r.status, 200, "paid mint envelope: {}", r.body);
        assert!(!r.needs_payment());
        assert_eq!(r.body["method"], "verify_handoff", "Stylus-bound envelope");

        // 5 — audit feed reflects the whole loop (intent + gate + vault + relay)
        let r = dispatch_okx(&router, &inv("audit_feed", "okx-5", Value::Null))
            .await
            .expect("feed dispatch");
        assert_eq!(r.status, 200);
        assert!(
            r.body.as_array().map(|a| a.len() >= 4).unwrap_or(false),
            "feed shows the full transcript"
        );
    }

    /// A burned/absent receipt still 402s through the OKX edge (gate enforced).
    #[tokio::test]
    async fn okx_replayed_receipt_bounces() {
        let router = handoff_router(HandoffState::new());
        let announced = dispatch_okx(&router, &inv("intent_announce", "a", Value::Null))
            .await
            .unwrap();
        let intent = announced.body;
        let intent_id = intent["intent_id"].as_str().unwrap().to_string();
        let settled = dispatch_okx(
            &router,
            &inv("payment_settlement", "s", json!({ "intent_id": intent_id })),
        )
        .await
        .unwrap();
        let receipt_id = settled.body["receipt_id"].as_str().unwrap().to_string();
        let mint = |rid: &str| {
            inv(
                "credential_minting",
                "m",
                json!({ "intent": intent.clone(), "receipt_id": rid }),
            )
        };
        // First redemption clears.
        assert_eq!(
            dispatch_okx(&router, &mint(&receipt_id))
                .await
                .unwrap()
                .status,
            200
        );
        // Replay of the burned receipt bounces.
        let r = dispatch_okx(&router, &mint(&receipt_id)).await.unwrap();
        assert_eq!(r.status, 402);
        assert!(r.needs_payment());
    }

    #[tokio::test]
    async fn unknown_capability_errors() {
        let router = handoff_router(HandoffState::new());
        assert!(dispatch_okx(&router, &inv("nope", "x", Value::Null))
            .await
            .is_err());
    }
}
