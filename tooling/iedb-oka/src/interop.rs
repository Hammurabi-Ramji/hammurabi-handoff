//! OKX marketplace ↔ internal vocabulary interop.
//!
//! The OKX.AI storefront speaks its own request/response shapes; the
//! Hammurabi Handoff pipeline speaks `AgentIntent` + settlement envelopes.
//! This module is the translation layer — pure data, no network, no keys.
//!
//! It intentionally does NOT depend on the `hammurabi-handoff` crate: coupling
//! the OKX edge to the sovereign pipeline's internal types would let
//! marketplace schema churn reach the vault. Instead it produces/consumes
//! transport-neutral JSON (`serde_json::Value`) at the boundary, which the
//! Handoff HTTP surface already accepts.

use serde::{Deserialize, Serialize};

use crate::{OkaError, Result};

/// An OKX.AI marketplace invocation of one of our advertised capabilities.
///
/// This is what arrives from the distribution layer; we normalize it into a
/// plain intent payload for `POST /agent/announce` / `POST /execute`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OkxInvocation {
    /// The capability id being invoked (matches a key in the ASP manifest).
    pub capability: String,
    /// OKX-side correlation id, echoed back in the result for the storefront.
    pub request_id: String,
    /// The capability arguments, verbatim.
    pub params: serde_json::Value,
}

impl OkxInvocation {
    /// Parse from marketplace JSON.
    pub fn from_json(text: &str) -> Result<Self> {
        serde_json::from_str(text).map_err(|e| OkaError::Schema(e.to_string()))
    }

    /// Extract the intent payload to hand to the Handoff surface. For the
    /// `credential_minting` capability the params *are* the agent intent; for
    /// others we pass them through unchanged.
    pub fn intent_payload(&self) -> serde_json::Value {
        self.params.clone()
    }
}

/// Our result, shaped for the OKX storefront to render. Wraps whatever the
/// Handoff surface returned (a 402 terms body, a settlement envelope, etc.)
/// and tags it with the marketplace correlation id + HTTP-ish status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OkxResult {
    pub request_id: String,
    /// Mirrors the Handoff HTTP status (200 settled, 402 payment required, …).
    pub status: u16,
    pub body: serde_json::Value,
}

impl OkxResult {
    /// Build a result echoing an invocation's correlation id.
    pub fn for_invocation(inv: &OkxInvocation, status: u16, body: serde_json::Value) -> Self {
        Self {
            request_id: inv.request_id.clone(),
            status,
            body,
        }
    }

    /// True when the storefront should surface an x402 payment prompt.
    pub fn needs_payment(&self) -> bool {
        self.status == 402
    }

    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).map_err(|e| OkaError::Schema(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn invocation_round_trips_and_extracts_payload() {
        let inv = OkxInvocation::from_json(
            r#"{ "capability": "credential_minting", "request_id": "okx-42",
                 "params": { "intent_id": "01JZ", "action": "execute_limit_order" } }"#,
        )
        .expect("parse");
        assert_eq!(inv.capability, "credential_minting");
        assert_eq!(inv.intent_payload()["intent_id"], "01JZ");
    }

    #[test]
    fn result_flags_payment_required() {
        let inv = OkxInvocation {
            capability: "credential_minting".into(),
            request_id: "okx-7".into(),
            params: json!({}),
        };
        let r = OkxResult::for_invocation(&inv, 402, json!({ "error": "payment_required" }));
        assert!(r.needs_payment());
        assert_eq!(r.request_id, "okx-7");

        let ok = OkxResult::for_invocation(&inv, 200, json!({ "method": "verify_handoff" }));
        assert!(!ok.needs_payment());
    }
}
