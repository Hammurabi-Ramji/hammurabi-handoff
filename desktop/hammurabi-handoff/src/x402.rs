//! Phase 2 — the x402 intercept middleware (the Monetization Gate).
//!
//! Every execution request must pass this Axum middleware before it can
//! reach the Handshake vault. Unpaid requests are refused with
//! `HTTP 402 Payment Required` plus machine-readable payment terms, and the
//! refusal is narrated to RAMesh so the operator sees the gate working.
//!
//! Payment resolution is **mocked locally** (this is a sovereign build — no
//! external payment rail): [`X402Gate::mock_settle`] mints a single-use
//! [`PaymentReceipt`], which the client then presents in the
//! [`RECEIPT_HEADER`]. Receipts are intent-bound and burned on redemption,
//! so a receipt can neither be replayed nor spent on a different intent.

use std::collections::HashMap;
use std::sync::RwLock;

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;
use ulid::Ulid;

use crate::mesh::MeshEventKind;
use crate::server::HandoffState;
use crate::settlement_backend::{SettlementBackend, SettlementError};

/// The exact Phase-2 refusal line (Legible Autonomy narrative string — do
/// not reword).
pub const X402_RESTRICTION_NOTICE: &str = "Action restricted. x402 payment of 0.50 USDC required.";

/// The gate's IRC handle.
pub const GATE_HANDLE: &str = "gate/x402";

/// Execution fee: 0.50 USDC in atomic units (USDC has 6 decimals).
pub const EXECUTION_FEE_MICRO_USDC: u64 = 500_000;

/// Header carrying a settled receipt id on a retried execution request.
pub const RECEIPT_HEADER: &str = "x-402-receipt";

/// Header binding the request to the intent the receipt was settled for.
pub const INTENT_HEADER: &str = "x-intent-id";

/// Machine-readable payment terms returned with every 402 (loosely follows
/// the x402 `accepts` shape; everything is local/stubbed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentTerms {
    pub x402_version: u8,
    /// `exact` — pay exactly the quoted amount.
    pub scheme: String,
    /// The (stubbed) settlement network.
    pub network: String,
    pub asset: String,
    /// Atomic units (6-decimal USDC): 500_000 = 0.50 USDC.
    pub amount_micro: u64,
    /// Human-readable rendering for the mesh/UI.
    pub amount_display: String,
    /// Where the payment goes — the local vault treasury.
    pub pay_to: String,
    /// The intent this quote covers (empty if the request never named one).
    pub intent_id: String,
    /// How to settle in this build.
    pub resolution: String,
}

impl PaymentTerms {
    fn quote(intent_id: &str) -> Self {
        Self {
            x402_version: 1,
            scheme: "exact".to_string(),
            network: "arbitrum-stylus-local-stub".to_string(),
            asset: "USDC".to_string(),
            amount_micro: EXECUTION_FEE_MICRO_USDC,
            amount_display: "0.50 USDC".to_string(),
            pay_to: "vault/handshake@127.0.0.1".to_string(),
            intent_id: intent_id.to_string(),
            resolution: "POST /x402/settle {\"intent_id\": ...}".to_string(),
        }
    }
}

/// Proof that the x402 fee cleared for one specific intent. Single-use:
/// the gate burns it at redemption.
///
/// This type is only ever constructed by [`X402Gate`] — holding one is the
/// type-level precondition for the Handshake vault to mint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentReceipt {
    pub receipt_id: String,
    pub intent_id: String,
    pub amount_micro_usdc: u64,
    /// Unix seconds at which the (mock) payment cleared.
    pub cleared_at: i64,
}

/// Gate failures — all of them resolve to a fresh 402 at the HTTP edge.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum X402Error {
    #[error("unknown or already-redeemed receipt")]
    UnknownReceipt,
    #[error("receipt is bound to a different intent")]
    IntentMismatch,
}

/// The payment gate: an in-memory treasury of settled, unredeemed receipts,
/// plus the [`SettlementBackend`] that produces them (mock by default; a live
/// x402 facilitator under the `x402-live` feature).
pub struct X402Gate {
    receipts: RwLock<HashMap<String, PaymentReceipt>>,
    backend: SettlementBackend,
}

impl X402Gate {
    pub fn new() -> Self {
        Self::with_backend(SettlementBackend::default())
    }

    /// Build a gate over a specific settlement backend.
    pub fn with_backend(backend: SettlementBackend) -> Self {
        Self {
            receipts: RwLock::new(HashMap::new()),
            backend,
        }
    }

    /// Quote the payment terms for an intent.
    pub fn terms_for(&self, intent_id: &str) -> PaymentTerms {
        PaymentTerms::quote(intent_id)
    }

    /// Settle the fee for `intent_id` through the configured backend and record
    /// the resulting single-use receipt. `payment` is the client's x402
    /// payload — required by the live facilitator backend, ignored by the mock.
    pub async fn settle(
        &self,
        intent_id: &str,
        payment: Option<&Value>,
        now: i64,
    ) -> Result<PaymentReceipt, SettlementError> {
        let receipt = self.backend.settle(intent_id, payment, now).await?;
        self.receipts
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(receipt.receipt_id.clone(), receipt.clone());
        Ok(receipt)
    }

    /// **Mock payment resolution.** Settles the 0.50 USDC fee locally and
    /// mints a single-use receipt bound to `intent_id`. In a live build this
    /// is where an actual x402 settlement proof would be verified.
    pub fn mock_settle(&self, intent_id: &str, now: i64) -> PaymentReceipt {
        let receipt = PaymentReceipt {
            receipt_id: Ulid::new().to_string(),
            intent_id: intent_id.to_string(),
            amount_micro_usdc: EXECUTION_FEE_MICRO_USDC,
            cleared_at: now,
        };
        self.receipts
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(receipt.receipt_id.clone(), receipt.clone());
        receipt
    }

    /// Redeem a receipt for `intent_id`. The receipt is **burned** on any
    /// redemption attempt that gets far enough to locate it — strict
    /// single-presentation semantics (anti-replay).
    pub fn redeem(&self, receipt_id: &str, intent_id: &str) -> Result<PaymentReceipt, X402Error> {
        let receipt = self
            .receipts
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(receipt_id)
            .ok_or(X402Error::UnknownReceipt)?;
        if receipt.intent_id != intent_id {
            return Err(X402Error::IntentMismatch);
        }
        Ok(receipt)
    }
}

impl Default for X402Gate {
    fn default() -> Self {
        Self::new()
    }
}

/// The Axum middleware guard on `/execute`.
///
/// - No [`RECEIPT_HEADER`] → `402` with payment terms, and the exact
///   restriction notice is posted to RAMesh.
/// - Receipt present but unknown / replayed / bound to a different intent →
///   `402` with fresh terms (and a distinct mesh line naming the reason).
/// - Receipt redeems cleanly → the burned [`PaymentReceipt`] is injected
///   into request extensions and the request proceeds to the vault.
pub async fn payment_guard(
    State(state): State<HandoffState>,
    mut req: Request,
    next: Next,
) -> Response {
    // Read both headers into owned values immediately — a helper closure
    // borrowing `req` would live across the `next.run(req).await` below and
    // make this future `!Send` (`Body` is `!Sync`), failing the middleware
    // Service bound.
    let intent_id = req
        .headers()
        .get(INTENT_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
        .unwrap_or_default();
    let receipt_id = req
        .headers()
        .get(RECEIPT_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    let Some(receipt_id) = receipt_id else {
        // Unpaid request: refuse, quote terms, narrate.
        state.mesh.post(
            GATE_HANDLE,
            MeshEventKind::Restriction,
            X402_RESTRICTION_NOTICE,
        );
        return payment_required(state.gate.terms_for(&intent_id));
    };

    match state.gate.redeem(&receipt_id, &intent_id) {
        Ok(receipt) => {
            req.extensions_mut().insert(receipt);
            next.run(req).await
        }
        Err(e) => {
            state.mesh.post(
                GATE_HANDLE,
                MeshEventKind::Restriction,
                &format!("Receipt {receipt_id} refused: {e}. {X402_RESTRICTION_NOTICE}"),
            );
            payment_required(state.gate.terms_for(&intent_id))
        }
    }
}

fn payment_required(terms: PaymentTerms) -> Response {
    (
        StatusCode::PAYMENT_REQUIRED,
        Json(json!({
            "error": "payment_required",
            "accepts": [terms],
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settle_then_redeem_burns_the_receipt() {
        let gate = X402Gate::new();
        let receipt = gate.mock_settle("intent-1", 1_000);
        assert_eq!(receipt.amount_micro_usdc, EXECUTION_FEE_MICRO_USDC);

        let redeemed = gate
            .redeem(&receipt.receipt_id, "intent-1")
            .expect("first redemption clears");
        assert_eq!(redeemed.receipt_id, receipt.receipt_id);

        // Replay must fail — the receipt is burned.
        assert_eq!(
            gate.redeem(&receipt.receipt_id, "intent-1"),
            Err(X402Error::UnknownReceipt)
        );
    }

    #[test]
    fn receipt_is_intent_bound() {
        let gate = X402Gate::new();
        let receipt = gate.mock_settle("intent-1", 1_000);
        assert_eq!(
            gate.redeem(&receipt.receipt_id, "intent-OTHER"),
            Err(X402Error::IntentMismatch)
        );
        // Strict single-presentation: the mismatch attempt burned it too.
        assert_eq!(
            gate.redeem(&receipt.receipt_id, "intent-1"),
            Err(X402Error::UnknownReceipt)
        );
    }

    #[test]
    fn terms_quote_the_exact_fee() {
        let gate = X402Gate::new();
        let terms = gate.terms_for("intent-1");
        assert_eq!(terms.amount_micro, 500_000);
        assert_eq!(terms.amount_display, "0.50 USDC");
        assert_eq!(terms.scheme, "exact");
    }
}
