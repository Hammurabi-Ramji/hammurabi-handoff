//! Phase 3 — the Hammurabi Handshake JIT credential vault.
//!
//! Triggers **only** after the x402 gate has cleared: [`mint`] takes a
//! [`PaymentReceipt`](crate::x402::PaymentReceipt) by reference, and the
//! gate is the sole constructor of that type — payment-before-credential is
//! enforced at the type level, not by convention.
//!
//! ## What "ephemeral, payload-bound" means here
//! - A **fresh Ed25519 keypair is generated per mint** inside this function
//!   and dropped before it returns. The seed is zeroized on drop
//!   (`iedb-crypto::Identity`). The secret key never exists outside this
//!   stack frame — there is nothing to persist, exfiltrate, or reuse.
//! - The signature covers `canonical(payload) ‖ 0x00 ‖ issued_at` (RFC 8785
//!   profile via `iedb-crypto::jcs`), so the credential is bound to exactly
//!   one payload and one instant. Changing a single field — or the
//!   timestamp — invalidates it.
//! - The credential expires **60 seconds** after `issued_at`
//!   ([`CREDENTIAL_TTL_SECS`]). Time is caller-supplied on every path
//!   (workspace `jcs` discipline): the vault never reads an ambient clock.
//!
//! ## Signature chaos testing
//! Before a credential is released, the vault attacks its own signature:
//! honest verification must pass, and tampered payloads, bit-flipped
//! signatures, and wrong verifying keys must all be rejected. If *any*
//! probe misbehaves, the credential is destroyed and [`VaultError::Chaos`]
//! is returned. This turns "the crypto library works" from an assumption
//! into a per-mint proof.

use iedb_crypto::crypto::{blake3_hash, verify_bytes, Identity, PublicKeyBytes};
use iedb_crypto::jcs;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use ulid::Ulid;

use crate::x402::PaymentReceipt;

/// The exact Phase-3 confirmation line (Legible Autonomy narrative string —
/// do not reword).
pub const CREDENTIAL_MINTED_NOTICE: &str =
    "Payment confirmed. Ephemeral Handshake credential minted.";

/// The vault's IRC handle.
pub const VAULT_HANDLE: &str = "vault/handshake";

/// Strict credential Time-To-Live.
pub const CREDENTIAL_TTL_SECS: i64 = 60;

/// Random bit-flip trials run against the signature during chaos testing.
pub const CHAOS_SIGNATURE_TRIALS: u32 = 8;

/// Outcome of the pre-release chaos probes. All fields must read as
/// "attack rejected" for the credential to exist at all — the report is
/// carried on the credential as an audit artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChaosReport {
    /// The honest (payload, timestamp, signature) triple verified.
    pub honest_verified: bool,
    /// A mutated payload was rejected.
    pub tampered_payload_rejected: bool,
    /// A timestamp shift (±1s) was rejected.
    pub shifted_timestamp_rejected: bool,
    /// Every one of [`CHAOS_SIGNATURE_TRIALS`] random single-bit signature
    /// flips was rejected.
    pub signature_bitflips_rejected: u32,
    /// Verification under a freshly generated wrong key was rejected.
    pub wrong_key_rejected: bool,
}

impl ChaosReport {
    fn all_clear(&self) -> bool {
        self.honest_verified
            && self.tampered_payload_rejected
            && self.shifted_timestamp_rejected
            && self.signature_bitflips_rejected == CHAOS_SIGNATURE_TRIALS
            && self.wrong_key_rejected
    }
}

/// The minted credential. Contains **only public material** — the signing
/// key was dropped (and its seed zeroized) before this struct was built.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EphemeralCredential {
    pub credential_id: String,
    /// The intent this credential authorizes — copied from the receipt.
    pub intent_id: String,
    /// The burned x402 receipt that unlocked the mint.
    pub receipt_id: String,
    /// Ephemeral Ed25519 verifying key, hex.
    pub public_key_hex: String,
    /// 64-byte Ed25519 signature over `canonical(payload) ‖ 0x00 ‖ issued_at`, hex.
    pub signature_hex: String,
    /// BLAKE3 digest of the canonical payload, hex — quick binding check.
    pub payload_digest_hex: String,
    /// Unix seconds. The signature's bound timestamp.
    pub issued_at: i64,
    /// `issued_at + CREDENTIAL_TTL_SECS`, structural.
    pub expires_at: i64,
    /// Audit artifact from the pre-release chaos probes.
    pub chaos: ChaosReport,
}

#[derive(Debug, Error)]
pub enum VaultError {
    /// The payload could not be canonicalized (e.g. contains a float) —
    /// an unverifiable payload is refused outright.
    #[error("payload rejected by canonicalization: {0}")]
    Canonicalization(String),
    /// A chaos probe misbehaved; the credential was destroyed.
    #[error("chaos testing failed — credential destroyed: {0:?}")]
    Chaos(ChaosReport),
    /// The credential is outside its validity window.
    #[error("credential expired or not yet valid (issued_at={issued_at}, expires_at={expires_at}, now={now})")]
    OutsideValidity {
        issued_at: i64,
        expires_at: i64,
        now: i64,
    },
    /// The credential's structural TTL was tampered with.
    #[error("credential TTL is not the strict {CREDENTIAL_TTL_SECS}s window")]
    MalformedTtl,
    /// The presented payload does not match the credential's digest.
    #[error("payload digest mismatch — credential is bound to a different payload")]
    DigestMismatch,
    /// Hex fields could not be decoded to key/signature material.
    #[error("malformed credential material: {0}")]
    Malformed(String),
    /// Ed25519 verification failed.
    #[error("signature invalid")]
    SignatureInvalid,
}

/// Mint an ephemeral, payload-bound credential. `now` is the caller's clock
/// (unix seconds) and becomes the signature-bound `issued_at`.
///
/// Requires a [`PaymentReceipt`] — only the x402 gate can produce one, so
/// this function is unreachable for unpaid requests.
pub fn mint(
    payload: &Value,
    receipt: &PaymentReceipt,
    now: i64,
) -> Result<EphemeralCredential, VaultError> {
    // 1. Canonicalize and digest the payload (refuse unverifiable payloads).
    let canonical =
        jcs::canonicalize(payload).map_err(|e| VaultError::Canonicalization(e.to_string()))?;
    let digest = blake3_hash(&canonical);

    // 2. Ephemeral keypair — lives only inside this frame.
    let identity = Identity::generate();
    let public_key = identity.public_key();

    // 3. Payload-bound signature: canonical(payload) ‖ 0x00 ‖ now.
    let signature = jcs::sign_payload(&identity, payload, now)
        .map_err(|e| VaultError::Canonicalization(e.to_string()))?;

    // 4. Chaos testing — attack the signature before trusting it.
    let chaos = chaos_test(&public_key, payload, now, &signature);
    if !chaos.all_clear() {
        return Err(VaultError::Chaos(chaos));
    }

    // 5. Release public material only. `identity` drops here; its seed is
    //    zeroized. The vault retains nothing.
    Ok(EphemeralCredential {
        credential_id: Ulid::new().to_string(),
        intent_id: receipt.intent_id.clone(),
        receipt_id: receipt.receipt_id.clone(),
        public_key_hex: hex::encode(public_key),
        signature_hex: hex::encode(signature),
        payload_digest_hex: hex::encode(digest),
        issued_at: now,
        expires_at: now + CREDENTIAL_TTL_SECS,
        chaos,
    })
}

/// Verify a credential against a presented payload at caller time `now`.
///
/// Checks, in order: structural TTL, validity window, payload digest
/// binding, then the Ed25519 signature over the canonical form.
pub fn verify(
    credential: &EphemeralCredential,
    payload: &Value,
    now: i64,
) -> Result<(), VaultError> {
    // Structural TTL — a credential claiming a longer life is malformed.
    if credential.expires_at != credential.issued_at + CREDENTIAL_TTL_SECS {
        return Err(VaultError::MalformedTtl);
    }
    // Validity window (strict: not before issued_at, not at/after expiry).
    if now < credential.issued_at || now >= credential.expires_at {
        return Err(VaultError::OutsideValidity {
            issued_at: credential.issued_at,
            expires_at: credential.expires_at,
            now,
        });
    }
    // Payload binding.
    let canonical =
        jcs::canonicalize(payload).map_err(|e| VaultError::Canonicalization(e.to_string()))?;
    if hex::encode(blake3_hash(&canonical)) != credential.payload_digest_hex {
        return Err(VaultError::DigestMismatch);
    }
    // Signature.
    let public_key: PublicKeyBytes = decode_fixed::<32>(&credential.public_key_hex)?;
    let signature = decode_fixed::<64>(&credential.signature_hex)?;
    jcs::verify_payload(&public_key, payload, credential.issued_at, &signature)
        .map_err(|_| VaultError::SignatureInvalid)
}

/// Decode a fixed-length hex field.
fn decode_fixed<const N: usize>(s: &str) -> Result<[u8; N], VaultError> {
    let bytes = hex::decode(s).map_err(|e| VaultError::Malformed(e.to_string()))?;
    bytes
        .try_into()
        .map_err(|_| VaultError::Malformed(format!("expected {N} bytes")))
}

/// Attack the freshly minted signature from four directions. Every probe
/// must land exactly as Ed25519 promises, or the mint aborts.
fn chaos_test(
    public_key: &PublicKeyBytes,
    payload: &Value,
    timestamp: i64,
    signature: &[u8; 64],
) -> ChaosReport {
    // Probe 0 — the honest triple verifies.
    let honest_verified = jcs::verify_payload(public_key, payload, timestamp, signature).is_ok();

    // Probe 1 — mutate the payload. For objects we inject a marker field;
    // for any other shape we wrap it, which also changes the canonical form.
    let tampered = match payload {
        Value::Object(map) => {
            let mut map = map.clone();
            map.insert("__chaos__".to_string(), Value::from(1u64));
            Value::Object(map)
        }
        other => serde_json::json!({ "__chaos__": other.clone() }),
    };
    let tampered_payload_rejected =
        jcs::verify_payload(public_key, &tampered, timestamp, signature).is_err();

    // Probe 2 — shift the bound timestamp by one second either way.
    let shifted_timestamp_rejected =
        jcs::verify_payload(public_key, payload, timestamp + 1, signature).is_err()
            && jcs::verify_payload(public_key, payload, timestamp - 1, signature).is_err();

    // Probe 3 — random single-bit flips in the signature itself.
    let mut rng = rand::thread_rng();
    let mut signature_bitflips_rejected = 0u32;
    for _ in 0..CHAOS_SIGNATURE_TRIALS {
        let bit = rng.gen_range(0..(64 * 8));
        let mut mutated = *signature;
        mutated[bit / 8] ^= 1 << (bit % 8);
        if jcs::verify_payload(public_key, payload, timestamp, &mutated).is_err() {
            signature_bitflips_rejected += 1;
        }
    }

    // Probe 4 — a wrong verifying key must reject the honest signature.
    let stranger = Identity::generate();
    let wrong_key_rejected = verify_bytes(
        &stranger.public_key(),
        &jcs::canonical_message(payload, timestamp).unwrap_or_default(),
        signature,
    )
    .is_err();

    ChaosReport {
        honest_verified,
        tampered_payload_rejected,
        shifted_timestamp_rejected,
        signature_bitflips_rejected,
        wrong_key_rejected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent::AgentIntent;
    use crate::x402::X402Gate;

    fn paid_receipt(intent: &AgentIntent) -> PaymentReceipt {
        X402Gate::new().mock_settle(&intent.intent_id, 1_000)
    }

    #[test]
    fn mint_produces_a_verifiable_60s_credential() {
        let intent = AgentIntent::mock_eth_limit_order();
        let payload = intent.payload();
        let receipt = paid_receipt(&intent);

        let cred = mint(&payload, &receipt, 1_000).expect("mint succeeds");
        assert_eq!(cred.expires_at - cred.issued_at, CREDENTIAL_TTL_SECS);
        assert!(cred.chaos.all_clear(), "chaos report must be all-clear");

        // Valid across the window…
        verify(&cred, &payload, 1_000).expect("valid at issue");
        verify(&cred, &payload, 1_059).expect("valid at t+59");
        // …dead at exactly t+60 and beyond.
        assert!(matches!(
            verify(&cred, &payload, 1_060),
            Err(VaultError::OutsideValidity { .. })
        ));
        // …and unusable before issue (no pre-dating).
        assert!(matches!(
            verify(&cred, &payload, 999),
            Err(VaultError::OutsideValidity { .. })
        ));
    }

    #[test]
    fn credential_is_payload_bound() {
        let intent = AgentIntent::mock_eth_limit_order();
        let payload = intent.payload();
        let receipt = paid_receipt(&intent);
        let cred = mint(&payload, &receipt, 1_000).expect("mint");

        // Same shape, one field nudged: $3,100.00 → $3,100.01.
        let mut other = intent.clone();
        other.order.limit_price_usd_cents += 1;
        assert!(matches!(
            verify(&cred, &other.payload(), 1_001),
            Err(VaultError::DigestMismatch)
        ));
    }

    #[test]
    fn tampered_signature_material_is_rejected() {
        let intent = AgentIntent::mock_eth_limit_order();
        let payload = intent.payload();
        let receipt = paid_receipt(&intent);
        let mut cred = mint(&payload, &receipt, 1_000).expect("mint");

        // Corrupt one hex nibble of the signature.
        let mut sig = cred.signature_hex.clone().into_bytes();
        sig[0] = if sig[0] == b'0' { b'1' } else { b'0' };
        cred.signature_hex = String::from_utf8(sig).expect("hex stays utf8");
        assert!(matches!(
            verify(&cred, &payload, 1_001),
            Err(VaultError::SignatureInvalid)
        ));
    }

    #[test]
    fn stretched_ttl_is_structurally_rejected() {
        let intent = AgentIntent::mock_eth_limit_order();
        let payload = intent.payload();
        let receipt = paid_receipt(&intent);
        let mut cred = mint(&payload, &receipt, 1_000).expect("mint");

        cred.expires_at += 3_600; // attacker grants themselves an hour
        assert!(matches!(
            verify(&cred, &payload, 1_001),
            Err(VaultError::MalformedTtl)
        ));
    }

    #[test]
    fn float_payloads_are_refused() {
        let intent = AgentIntent::mock_eth_limit_order();
        let receipt = paid_receipt(&intent);
        let floaty = serde_json::json!({ "price": 3100.0 });
        assert!(matches!(
            mint(&floaty, &receipt, 1_000),
            Err(VaultError::Canonicalization(_))
        ));
    }
}
