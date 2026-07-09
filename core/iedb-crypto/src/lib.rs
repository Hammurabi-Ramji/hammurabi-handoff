//! # iedb-crypto — Cryptographic Bedrock (MVP slice)
//!
//! The real BSM-C mechanics the whole system rests on, matching the contract the
//! top-down pillars already prove against:
//! - [`crypto`] — Ed25519 identity, BLAKE3 + SHA-256 hashing, X25519 derivation,
//!   ChaCha20-Poly1305 + Argon2 keystore, seed zeroization; unified [`CryptoError`].
//! - [`merkle`] / [`mmr`] — the append-only Merkle Mountain Range accumulator
//!   behind the unified ledger's mathematical provability.
//!
//! Scope for this public release: only the surface the Hammurabi Handoff vault
//! depends on — Ed25519 identity, RFC 8785 JCS canonicalization, BLAKE3/SHA-256
//! hashing, the ChaCha20-Poly1305 keystore, and the Merkle accumulator. The
//! proprietary research-grade modules (`zk`, `gates`, `circuit`, `verifier`,
//! WASM sandbox, attestation) are **not part of this repository**; they remain
//! private to the IEDB workspace.
//!
//! **Phase 2 retirement:** the `handshake` module — the HTTP 402 / axum gateway
//! — was *removed* (not merely undeclared). Its Ed25519 verification logic now
//! lives on the loopback IPC bus (`iedb-transport::ipc_bus::auth`), and the
//! `axum`/`tower`/`tower-http` dependencies are gone. The bedrock signs and
//! verifies (see [`jcs`]); it does not serve.

#![forbid(unsafe_code)]

pub mod aead;
pub mod crypto;
pub mod jcs;
pub mod merkle;
pub mod mmr;
// Purified out of quarantine (Item 3): zero-IO, deterministic, zero external deps.
pub mod tokens;

pub use crypto::{
    blake3_hash, sha256, verify_bytes, verify_signature, CryptoError, CryptoResult,
    EncryptedKeystore, Identity, PublicKeyBytes,
};
pub use merkle::{CompactProof, MerkleForest, MmrError, Peak};

/// Version of the cryptographic bedrock crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
