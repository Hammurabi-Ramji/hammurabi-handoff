//! Settlement backends behind the x402 gate.
//!
//! This is the seam that lets the `/x402/settle` path swap the local mock for
//! a **live x402 facilitator** without touching the gate's receipt ledger, the
//! vault, or the request flow. [`X402Gate::settle`](crate::x402::X402Gate::settle)
//! calls a [`SettlementBackend`]; the backend either mints a receipt locally
//! (offline, deterministic — the default) or drives a real facilitator over
//! HTTP.
//!
//! ## Flow difference (why this isn't a drop-in swap)
//! The mock settles from a bare `intent_id`. Real x402 does not: the **client**
//! constructs and signs an x402 `PaymentPayload`, the resource server hands it
//! to the facilitator's `/verify` then `/settle` endpoints, and only a
//! facilitator-confirmed settlement yields a receipt. So the live path requires
//! the client to present that payload on `POST /x402/settle`
//! (`{ "intent_id": …, "payment": <PaymentPayload> }`); the mock ignores it.
//!
//! ## Honesty boundary
//! The **seam** (this enum, the gate wiring, the flow) is real and tested. The
//! [`facilitator`] client is real HTTP plumbing coded to the documented x402
//! facilitator protocol, but it is **feature-gated (`x402-live`), off by
//! default, and unverified against any specific facilitator**. Before trusting
//! it: (1) confirm the request/response field names against your facilitator's
//! actual API — they vary across implementations; (2) round-trip it on a
//! testnet (e.g. base-sepolia) against real USDC before mainnet. Do not point
//! it at a live money rail on the strength of this code alone.

use serde_json::Value;
use ulid::Ulid;

use crate::x402::{PaymentReceipt, EXECUTION_FEE_MICRO_USDC};

/// Why a settlement attempt failed.
#[derive(Debug)]
pub enum SettlementError {
    /// The live backend needs a client x402 `PaymentPayload` the request did
    /// not carry.
    MissingPayment,
    /// The facilitator rejected the payment (invalid, expired, underpaid, …).
    Rejected(String),
    /// Transport- or protocol-level failure talking to the facilitator.
    Transport(String),
}

impl std::fmt::Display for SettlementError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SettlementError::MissingPayment => {
                write!(f, "live settlement requires a client x402 payment payload")
            }
            SettlementError::Rejected(why) => write!(f, "facilitator rejected payment: {why}"),
            SettlementError::Transport(why) => write!(f, "facilitator transport error: {why}"),
        }
    }
}

impl std::error::Error for SettlementError {}

/// How the gate settles the x402 fee. `Clone` is cheap: `Mock` is a unit and
/// the facilitator client is internally reference-counted.
#[derive(Clone, Default)]
pub enum SettlementBackend {
    /// Offline, deterministic: mints a receipt locally with no network I/O.
    /// The default — keeps the sovereign build fully self-contained.
    #[default]
    Mock,
    /// Live x402 facilitator over HTTP. Only constructible with `x402-live`.
    #[cfg(feature = "x402-live")]
    Facilitator(facilitator::FacilitatorClient),
}

impl SettlementBackend {
    /// Human-readable label for the RAMesh startup line — "if it isn't in
    /// the feed, it didn't happen" applies to which settlement backend is
    /// armed, not just to agent actions.
    pub fn label(&self) -> &'static str {
        match self {
            SettlementBackend::Mock => "mock (offline, deterministic)",
            #[cfg(feature = "x402-live")]
            SettlementBackend::Facilitator(_) => "live x402 facilitator",
        }
    }
}

/// Pick the backend the composition root (`app.rs`) should arm at startup.
///
/// Without `x402-live` this always returns [`SettlementBackend::Mock`] — the
/// facilitator variant doesn't exist in the type at all off that feature, so
/// there's no ambiguity to resolve. With `x402-live` compiled in, a live
/// facilitator is armed **only if** every required `HANDOFF_X402_*` env var
/// is set ([`facilitator::FacilitatorConfig::from_env`]); otherwise this
/// falls back to the mock rather than failing startup — enabling the feature
/// flag alone must never be enough to accidentally go live.
#[cfg(feature = "x402-live")]
pub fn from_env_or_default() -> SettlementBackend {
    match facilitator::FacilitatorConfig::from_env() {
        Ok(config) => {
            tracing::info!(
                base_url = %config.base_url,
                network = %config.network,
                "x402-live: HANDOFF_X402_* present — arming the live facilitator backend"
            );
            SettlementBackend::Facilitator(facilitator::FacilitatorClient::new(config))
        }
        Err(_) => {
            tracing::info!(
                "x402-live is compiled in but HANDOFF_X402_* env vars are not fully set — \
                 falling back to the mock settlement backend"
            );
            SettlementBackend::Mock
        }
    }
}

/// Mirrors the `x402-live` version above: without the feature, the mock is
/// the only backend that exists.
#[cfg(not(feature = "x402-live"))]
pub fn from_env_or_default() -> SettlementBackend {
    SettlementBackend::Mock
}

impl SettlementBackend {
    /// Settle the execution fee for `intent_id`, returning a single-use
    /// [`PaymentReceipt`] the gate will record. `payment` is the client's x402
    /// `PaymentPayload` (required by the live backend, ignored by the mock).
    pub async fn settle(
        &self,
        intent_id: &str,
        payment: Option<&Value>,
        now: i64,
    ) -> Result<PaymentReceipt, SettlementError> {
        match self {
            SettlementBackend::Mock => {
                let _ = &payment; // unused off the live path
                Ok(PaymentReceipt {
                    receipt_id: Ulid::new().to_string(),
                    intent_id: intent_id.to_string(),
                    amount_micro_usdc: EXECUTION_FEE_MICRO_USDC,
                    cleared_at: now,
                })
            }
            #[cfg(feature = "x402-live")]
            SettlementBackend::Facilitator(client) => {
                let payment = payment.ok_or(SettlementError::MissingPayment)?;
                client.settle_intent(intent_id, payment, now).await
            }
        }
    }
}

// ── Live x402 facilitator client (opt-in) ─────────────────────────────────
#[cfg(feature = "x402-live")]
pub mod facilitator {
    //! HTTP client for a live x402 facilitator (`/verify` + `/settle`).
    //!
    //! Schema follows the documented x402 facilitator protocol. Field names are
    //! the most common form; **verify them against your facilitator's API** —
    //! this module is the one place that must change if they differ.

    use serde::de::DeserializeOwned;
    use serde::{Deserialize, Serialize};
    use serde_json::{json, Value};
    use ulid::Ulid;

    use super::SettlementError;
    use crate::x402::{PaymentReceipt, EXECUTION_FEE_MICRO_USDC};

    /// Live facilitator configuration. Populate from the environment.
    #[derive(Clone, Debug)]
    pub struct FacilitatorConfig {
        /// Facilitator base URL, e.g. `https://x402.org/facilitator`.
        pub base_url: String,
        /// Network id the payment settles on, e.g. `base-sepolia`.
        pub network: String,
        /// USDC (or other asset) contract address on `network`.
        pub asset: String,
        /// Address that receives the fee.
        pub pay_to: String,
        /// The x402 scheme, typically `exact`.
        pub scheme: String,
        /// The resource being paid for (the `/execute` URL).
        pub resource: String,
        /// Optional bearer token for the facilitator.
        pub api_key: Option<String>,
    }

    impl FacilitatorConfig {
        /// Read config from `HANDOFF_X402_*` environment variables. Errors if a
        /// required one is missing so a live build fails loud, not silent.
        pub fn from_env() -> Result<Self, SettlementError> {
            let req = |k: &str| {
                std::env::var(k).map_err(|_| SettlementError::Transport(format!("missing env {k}")))
            };
            Ok(Self {
                base_url: req("HANDOFF_X402_FACILITATOR_URL")?,
                network: req("HANDOFF_X402_NETWORK")?,
                asset: req("HANDOFF_X402_ASSET")?,
                pay_to: req("HANDOFF_X402_PAY_TO")?,
                scheme: std::env::var("HANDOFF_X402_SCHEME").unwrap_or_else(|_| "exact".into()),
                resource: std::env::var("HANDOFF_X402_RESOURCE")
                    .unwrap_or_else(|_| "http://127.0.0.1:3402/execute".into()),
                api_key: std::env::var("HANDOFF_X402_API_KEY").ok(),
            })
        }
    }

    /// x402 payment requirements (the `accepts` entry the facilitator checks a
    /// payment against). Serialized into `/verify` and `/settle` requests.
    #[derive(Debug, Clone, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct PaymentRequirements {
        scheme: String,
        network: String,
        /// Atomic units, as a string per the x402 spec.
        max_amount_required: String,
        resource: String,
        description: String,
        mime_type: String,
        pay_to: String,
        max_timeout_seconds: u64,
        asset: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        extra: Option<Value>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct VerifyResponse {
        is_valid: bool,
        #[serde(default)]
        invalid_reason: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct SettleResponse {
        success: bool,
        /// On-chain settlement tx hash, when the facilitator reports one.
        #[serde(default)]
        transaction: Option<String>,
        #[serde(default)]
        error_reason: Option<String>,
    }

    /// A live x402 facilitator client. Cheap to clone (reqwest `Client` is
    /// internally reference-counted).
    #[derive(Clone)]
    pub struct FacilitatorClient {
        http: reqwest::Client,
        config: FacilitatorConfig,
    }

    impl FacilitatorClient {
        pub fn new(config: FacilitatorConfig) -> Self {
            Self {
                http: reqwest::Client::new(),
                config,
            }
        }

        /// Verify then settle the client's payment for `intent_id`. Returns a
        /// receipt only on a facilitator-confirmed settlement.
        pub async fn settle_intent(
            &self,
            intent_id: &str,
            payment: &Value,
            now: i64,
        ) -> Result<PaymentReceipt, SettlementError> {
            let requirements = self.requirements();

            let verify: VerifyResponse = self.post("/verify", payment, &requirements).await?;
            if !verify.is_valid {
                return Err(SettlementError::Rejected(
                    verify
                        .invalid_reason
                        .unwrap_or_else(|| "payment failed verification".into()),
                ));
            }

            let settle: SettleResponse = self.post("/settle", payment, &requirements).await?;
            if !settle.success {
                return Err(SettlementError::Rejected(
                    settle
                        .error_reason
                        .unwrap_or_else(|| "facilitator declined settlement".into()),
                ));
            }

            // `settle.transaction` is the on-chain proof. Surfaced in the log;
            // threading it into PaymentReceipt/OneShotEnvelope would extend
            // those types and is deliberately left to a follow-up.
            if let Some(tx) = settle.transaction {
                tracing::info!(intent_id, tx, "x402 facilitator settled on-chain");
            }

            Ok(PaymentReceipt {
                receipt_id: Ulid::new().to_string(),
                intent_id: intent_id.to_string(),
                amount_micro_usdc: EXECUTION_FEE_MICRO_USDC,
                cleared_at: now,
            })
        }

        fn requirements(&self) -> PaymentRequirements {
            PaymentRequirements {
                scheme: self.config.scheme.clone(),
                network: self.config.network.clone(),
                max_amount_required: EXECUTION_FEE_MICRO_USDC.to_string(),
                resource: self.config.resource.clone(),
                description: "Hammurabi Handoff — credential mint fee".into(),
                mime_type: "application/json".into(),
                pay_to: self.config.pay_to.clone(),
                max_timeout_seconds: 60,
                asset: self.config.asset.clone(),
                extra: None,
            }
        }

        async fn post<R: DeserializeOwned>(
            &self,
            path: &str,
            payment: &Value,
            requirements: &PaymentRequirements,
        ) -> Result<R, SettlementError> {
            let url = format!("{}{}", self.config.base_url.trim_end_matches('/'), path);
            let body = json!({
                "x402Version": 1,
                "paymentPayload": payment,
                "paymentRequirements": requirements,
            });
            let mut req = self.http.post(&url).json(&body);
            if let Some(key) = &self.config.api_key {
                req = req.bearer_auth(key);
            }
            let resp = req
                .send()
                .await
                .map_err(|e| SettlementError::Transport(e.to_string()))?;
            if !resp.status().is_success() {
                return Err(SettlementError::Transport(format!(
                    "facilitator {path} → HTTP {}",
                    resp.status()
                )));
            }
            resp.json::<R>()
                .await
                .map_err(|e| SettlementError::Transport(e.to_string()))
        }
    }
}

#[cfg(all(test, feature = "x402-live"))]
mod env_wiring_tests {
    use super::*;

    const VARS: &[&str] = &[
        "HANDOFF_X402_FACILITATOR_URL",
        "HANDOFF_X402_NETWORK",
        "HANDOFF_X402_ASSET",
        "HANDOFF_X402_PAY_TO",
    ];

    /// One test, not two: env vars are process-global, and `cargo test` runs
    /// tests on threads within one process, so two tests racing to set/unset
    /// the same vars would be flaky. Sequencing missing → present → cleaned
    /// up inside a single test avoids that entirely.
    #[test]
    fn from_env_or_default_falls_back_then_arms_on_full_config() {
        for k in VARS {
            std::env::remove_var(k);
        }

        assert!(
            matches!(from_env_or_default(), SettlementBackend::Mock),
            "missing HANDOFF_X402_* must fall back to Mock, not panic or half-configure"
        );

        std::env::set_var(
            "HANDOFF_X402_FACILITATOR_URL",
            "https://x402.example/facilitator",
        );
        std::env::set_var("HANDOFF_X402_NETWORK", "base-sepolia");
        std::env::set_var(
            "HANDOFF_X402_ASSET",
            "0x0000000000000000000000000000000000000001",
        );
        std::env::set_var(
            "HANDOFF_X402_PAY_TO",
            "0x0000000000000000000000000000000000000002",
        );

        assert!(
            matches!(from_env_or_default(), SettlementBackend::Facilitator(_)),
            "a fully-populated HANDOFF_X402_* set must arm the live facilitator"
        );

        for k in VARS {
            std::env::remove_var(k);
        }
    }
}
