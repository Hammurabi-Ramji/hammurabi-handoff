//! Phase 4 — settlement formatting (1Shot / Arbitrum Stylus prep).
//!
//! Packages the original intent payload plus the JIT Ed25519 credential
//! into the JSON envelope a 1Shot transaction endpoint would relay
//! gaslessly to an Arbitrum Stylus verifier contract.
//!
//! **Stubbed by design**: this module formats and returns the envelope —
//! it performs no network I/O of any kind. The `endpoint` field is a
//! `local://` URI precisely so that nothing can mistake it for a live
//! relay target. Wiring a real 1Shot API key and endpoint id is a
//! deliberate, later, human decision.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::vault::{EphemeralCredential, CREDENTIAL_TTL_SECS};
use crate::x402::PaymentReceipt;
use iedb_crypto::crypto::{blake3_hash, verify_bytes, PublicKeyBytes};
use iedb_crypto::jcs;

/// The settlement relay's IRC handle.
pub const RELAY_HANDLE: &str = "relay/1shot";

/// Stub 1Shot transaction endpoint — `local://` scheme guarantees it is
/// never routable.
pub const ONESHOT_ENDPOINT_STUB: &str = "local://1shot/transactions/hammurabi-handoff-stub/execute";

/// Placeholder Stylus verifier address (20 bytes, obviously non-live).
pub const STYLUS_VERIFIER_STUB_ADDRESS: &str = "0x0000000000000000000000000000000000005710";

/// Arbitrum One chain id — what the Stylus verifier would live on.
pub const ARBITRUM_ONE_CHAIN_ID: u64 = 42_161;

/// The verifier method the envelope targets.
pub const VERIFIER_METHOD: &str = "verify_handoff";

/// Everything the Stylus contract needs to re-run the Handshake
/// verification on-chain: the payload, the ephemeral verifying key, the
/// signature, and the validity window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementParams {
    /// The original agent intent, verbatim — the contract re-canonicalizes
    /// (RFC 8785) and checks the signature over
    /// `canonical(intent) ‖ 0x00 ‖ issued_at`.
    pub intent: Value,
    pub payload_digest_hex: String,
    pub signer_public_key_hex: String,
    pub signature_hex: String,
    pub issued_at: i64,
    pub expires_at: i64,
    /// The burned x402 receipt id — links settlement back to payment.
    pub receipt_id: String,
    pub fee_micro_usdc: u64,
}

/// The gasless broadcast envelope, 1Shot-shaped.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OneShotEnvelope {
    /// Envelope schema tag.
    pub api: String,
    /// Where this would be POSTed — `local://` stub, never routable.
    pub endpoint: String,
    /// 1Shot relays sponsor gas; the agent never holds ETH.
    pub broadcast_mode: String,
    pub chain_id: u64,
    pub contract_address: String,
    pub method: String,
    /// Credential id doubles as the idempotency key: one credential, one
    /// settlement, ever.
    pub idempotency_key: String,
    pub params: SettlementParams,
}

impl OneShotEnvelope {
    /// Package an intent payload + minted credential + burned receipt into
    /// a broadcast-ready envelope.
    pub fn package(
        intent_payload: &Value,
        credential: &EphemeralCredential,
        receipt: &PaymentReceipt,
    ) -> Self {
        Self {
            api: "1shot/v0-stub".to_string(),
            endpoint: ONESHOT_ENDPOINT_STUB.to_string(),
            broadcast_mode: "gasless-sponsored".to_string(),
            chain_id: ARBITRUM_ONE_CHAIN_ID,
            contract_address: STYLUS_VERIFIER_STUB_ADDRESS.to_string(),
            method: VERIFIER_METHOD.to_string(),
            idempotency_key: credential.credential_id.clone(),
            params: SettlementParams {
                intent: intent_payload.clone(),
                payload_digest_hex: credential.payload_digest_hex.clone(),
                signer_public_key_hex: credential.public_key_hex.clone(),
                signature_hex: credential.signature_hex.clone(),
                issued_at: credential.issued_at,
                expires_at: credential.expires_at,
                receipt_id: receipt.receipt_id.clone(),
                fee_micro_usdc: receipt.amount_micro_usdc,
            },
        }
    }

    /// The final broadcast JSON — what would go over the wire to 1Shot.
    pub fn to_broadcast_json(&self) -> Value {
        serde_json::to_value(self).expect("envelope is float-free and serializes")
    }

    /// Verify this envelope's credential exactly as the on-chain Stylus
    /// contract would (see [`SettlementParams::verify`] / [`verify_handoff`]).
    pub fn verify(&self, now: i64) -> Result<(), HandoffVerifyError> {
        self.params.verify(now)
    }
}

/// Failure modes of the Handoff verifier — the exact set an on-chain
/// `verify_handoff` would surface.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum HandoffVerifyError {
    /// The credential's structural TTL is not the strict window.
    #[error("credential TTL is not the strict {CREDENTIAL_TTL_SECS}s window")]
    MalformedTtl,
    /// `now` is outside `[issued_at, expires_at)`.
    #[error("outside validity window (issued_at={issued_at}, expires_at={expires_at}, now={now})")]
    OutsideValidity {
        issued_at: i64,
        expires_at: i64,
        now: i64,
    },
    /// The presented intent does not hash to the envelope's digest.
    #[error("payload digest mismatch — envelope intent differs from the signed one")]
    DigestMismatch,
    /// Ed25519 verification failed.
    #[error("signature invalid")]
    SignatureInvalid,
    /// A hex field could not be decoded to key/signature bytes.
    #[error("malformed material: {0}")]
    Malformed(String),
}

/// **Reference verifier** — the byte-level logic an Arbitrum Stylus
/// `verify_handoff` contract runs, hoisted out of the tests into a named
/// entry point. Operates solely on raw bytes so it is mechanically portable
/// to a `no_std` WASM contract: it pulls in no `serde`, no JSON, no host
/// types — just the RFC 8785 *canonical intent bytes*, the ephemeral verifying
/// key, the signature, and the validity window. (This host copy uses `std`
/// conveniences around the edges; the algorithm below does not.)
///
/// Checks, in order:
/// 1. structural TTL: `expires_at == issued_at + CREDENTIAL_TTL_SECS`;
/// 2. validity window: `issued_at <= now < expires_at`;
/// 3. Ed25519 over `canonical_intent ‖ 0x00 ‖ issued_at_le` — the exact
///    message the vault signed (`iedb_crypto::jcs`).
///
/// The caller supplies the canonical intent bytes; on-chain, the contract
/// re-canonicalizes the intent it was handed and passes the result here. This
/// function does **not** re-hash the payload digest — the signature already
/// binds the canonical bytes; the digest is a convenience checked by
/// [`SettlementParams::verify`].
pub fn verify_handoff(
    signer_public_key: &PublicKeyBytes,
    signature: &[u8; 64],
    canonical_intent: &[u8],
    issued_at: i64,
    expires_at: i64,
    now: i64,
) -> Result<(), HandoffVerifyError> {
    if expires_at != issued_at + CREDENTIAL_TTL_SECS {
        return Err(HandoffVerifyError::MalformedTtl);
    }
    if now < issued_at || now >= expires_at {
        return Err(HandoffVerifyError::OutsideValidity {
            issued_at,
            expires_at,
            now,
        });
    }
    // Reconstruct the signed message: canonical(intent) ‖ 0x00 ‖ issued_at_le.
    let mut msg = Vec::with_capacity(canonical_intent.len() + 1 + 8);
    msg.extend_from_slice(canonical_intent);
    msg.push(0);
    msg.extend_from_slice(&issued_at.to_le_bytes());
    verify_bytes(signer_public_key, &msg, signature)
        .map_err(|_| HandoffVerifyError::SignatureInvalid)
}

impl SettlementParams {
    /// Envelope-side wrapper over [`verify_handoff`]: decodes the hex fields,
    /// canonicalizes the carried intent, checks the payload digest binds, then
    /// runs the reference verifier. This is the bridge between the JSON
    /// envelope (serde world) and the raw-bytes contract logic. Proves the
    /// envelope is self-contained — everything needed to verify is inside it.
    pub fn verify(&self, now: i64) -> Result<(), HandoffVerifyError> {
        let public_key = decode_hex_array::<32>(&self.signer_public_key_hex)?;
        let signature = decode_hex_array::<64>(&self.signature_hex)?;
        let canonical = jcs::canonicalize(&self.intent)
            .map_err(|e| HandoffVerifyError::Malformed(e.to_string()))?;
        if hex::encode(blake3_hash(&canonical)) != self.payload_digest_hex {
            return Err(HandoffVerifyError::DigestMismatch);
        }
        verify_handoff(
            &public_key,
            &signature,
            &canonical,
            self.issued_at,
            self.expires_at,
            now,
        )
    }
}

/// Decode a fixed-length hex field into a byte array.
fn decode_hex_array<const N: usize>(s: &str) -> Result<[u8; N], HandoffVerifyError> {
    let bytes = hex::decode(s).map_err(|e| HandoffVerifyError::Malformed(e.to_string()))?;
    bytes
        .try_into()
        .map_err(|_| HandoffVerifyError::Malformed(format!("expected {N} bytes")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::AgentIntent;
    use crate::vault;
    use crate::x402::X402Gate;

    #[test]
    fn envelope_carries_a_verifiable_signature() {
        let intent = AgentIntent::mock_eth_limit_order();
        let payload = intent.payload();
        let receipt = X402Gate::new().mock_settle(&intent.intent_id, 2_000);
        let cred = vault::mint(&payload, &receipt, 2_000).expect("mint");

        let envelope = OneShotEnvelope::package(&payload, &cred, &receipt);

        // Verify via the named reference verifier (the Stylus `verify_handoff`
        // logic) straight off the envelope — proving it is self-contained.
        // Round-trip through broadcast JSON first to prove nothing but the
        // wire form is needed.
        let json = envelope.to_broadcast_json();
        let reparsed: OneShotEnvelope = serde_json::from_value(json).expect("envelope round-trips");
        reparsed.verify(2_030).expect("in-window envelope verifies");
    }

    #[test]
    fn verify_handoff_rejects_tamper_expiry_and_ttl_stretch() {
        let intent = AgentIntent::mock_eth_limit_order();
        let payload = intent.payload();
        let receipt = X402Gate::new().mock_settle(&intent.intent_id, 2_000);
        let cred = vault::mint(&payload, &receipt, 2_000).expect("mint");
        let envelope = OneShotEnvelope::package(&payload, &cred, &receipt);

        // Valid across the strict 60s window, dead at/after expiry.
        assert!(envelope.verify(2_000).is_ok(), "valid at issue");
        assert!(envelope.verify(2_059).is_ok(), "valid at t+59");
        assert_eq!(
            envelope.verify(2_060),
            Err(HandoffVerifyError::OutsideValidity {
                issued_at: 2_000,
                expires_at: 2_060,
                now: 2_060,
            })
        );
        assert!(matches!(
            envelope.verify(1_999),
            Err(HandoffVerifyError::OutsideValidity { .. })
        ));

        // Tampered intent → digest mismatch (envelope wrapper) before sig.
        let mut tampered = envelope.clone();
        tampered.params.intent["order"]["limit_price_usd_cents"] = serde_json::json!(310_001);
        assert_eq!(
            tampered.verify(2_030),
            Err(HandoffVerifyError::DigestMismatch)
        );

        // Stretched TTL → structural rejection.
        let mut stretched = envelope.clone();
        stretched.params.expires_at += 3_600;
        assert_eq!(
            stretched.verify(2_030),
            Err(HandoffVerifyError::MalformedTtl)
        );

        // Bit-flipped signature → SignatureInvalid at the raw verifier.
        let mut bad_sig = envelope.clone();
        let mut sig = bad_sig.params.signature_hex.clone().into_bytes();
        sig[0] = if sig[0] == b'0' { b'1' } else { b'0' };
        bad_sig.params.signature_hex = String::from_utf8(sig).unwrap();
        assert_eq!(
            bad_sig.verify(2_030),
            Err(HandoffVerifyError::SignatureInvalid)
        );
    }

    #[test]
    fn envelope_targets_the_stub_relay_only() {
        let intent = AgentIntent::mock_eth_limit_order();
        let payload = intent.payload();
        let receipt = X402Gate::new().mock_settle(&intent.intent_id, 2_000);
        let cred = vault::mint(&payload, &receipt, 2_000).expect("mint");
        let envelope = OneShotEnvelope::package(&payload, &cred, &receipt);

        assert!(envelope.endpoint.starts_with("local://"), "never routable");
        assert_eq!(envelope.broadcast_mode, "gasless-sponsored");
        assert_eq!(envelope.chain_id, ARBITRUM_ONE_CHAIN_ID);
    }

    #[test]
    fn envelope_contains_no_secret_material() {
        // Belt-and-braces: serialize the envelope and assert none of the
        // fields could hold a 32-byte seed — the vault never exposes one,
        // so the only 32/64-byte hex strings must be pubkey/digest/sig.
        let intent = AgentIntent::mock_eth_limit_order();
        let payload = intent.payload();
        let receipt = X402Gate::new().mock_settle(&intent.intent_id, 2_000);
        let cred = vault::mint(&payload, &receipt, 2_000).expect("mint");
        let json = OneShotEnvelope::package(&payload, &cred, &receipt).to_broadcast_json();
        let rendered = json.to_string();
        assert!(!rendered.contains("secret"));
        assert!(!rendered.contains("seed"));
        assert!(!rendered.contains("private"));
    }
}
