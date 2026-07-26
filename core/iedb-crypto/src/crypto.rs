//! `crypto` — the cryptographic bedrock.
//!
//! Real primitives the whole system rests on, matching the contract the
//! top-down pillars already prove against (`LiveCrypto`, `secure_email::identity`):
//! - **Ed25519** sign/verify ([`Identity`], [`verify_signature`]);
//! - **hashing** — BLAKE3 ([`blake3_hash`]) for the convergence/Merkle ledger and
//!   SHA-256 ([`sha256`]) for interop;
//! - **X25519 derivation** from an Ed25519 address (for sealed messages);
//! - **zeroization** — the secret seed is wiped on drop;
//! - **Argon2 keystore** — passphrase-encrypted seed at rest ([`EncryptedKeystore`]).
//!
//! Every fallible path returns [`CryptoError`]; zero `unwrap`/`expect`/`panic`.

use argon2::Argon2;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use curve25519_dalek::edwards::CompressedEdwardsY;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::{Digest, Sha256, Sha512};
use thiserror::Error;
use zeroize::Zeroize;

/// A raw Ed25519 public key — the sovereign address.
pub type PublicKeyBytes = [u8; 32];

/// The unified error for the cryptographic bedrock.
#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("invalid key: {0}")]
    InvalidKey(String),
    #[error("signature verification failed")]
    SignatureInvalid,
    #[error("keystore error: {0}")]
    Keystore(String),
    #[error("seal/open failed")]
    SealFailed,
    #[error("feature gated: {0}")]
    FeatureGated(String),
    #[error("token error: {0}")]
    Token(String),
    #[error("canonicalization rejected: {0}")]
    Canonicalization(String),
}

pub type CryptoResult<T> = Result<T, CryptoError>;

// ── Hashing ──────────────────────────────────────────────────────────

/// BLAKE3 digest — the ledger/convergence hash.
pub fn blake3_hash(bytes: &[u8]) -> [u8; 32] {
    *blake3::hash(bytes).as_bytes()
}

/// SHA-256 digest — for cross-ecosystem interop.
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(bytes);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

// ── Identity (Ed25519 + derived X25519) ──────────────────────────────

/// An Ed25519 keypair with a derived X25519 encryption key. The 32-byte seed is
/// zeroized on drop.
pub struct Identity {
    signing: SigningKey,
    seed: [u8; 32],
}

impl Identity {
    /// Generate a fresh identity from the OS CSPRNG.
    pub fn generate() -> Self {
        let mut seed = [0u8; 32];
        OsRng.fill_bytes(&mut seed);
        Self::from_seed(seed)
    }

    /// Reconstruct a deterministic identity from a 32-byte seed.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self {
            signing: SigningKey::from_bytes(&seed),
            seed,
        }
    }

    /// The Ed25519 public key (sovereign address).
    pub fn public_key(&self) -> PublicKeyBytes {
        self.signing.verifying_key().to_bytes()
    }

    /// Sign `msg` with the Ed25519 secret key.
    pub fn sign(&self, msg: &[u8]) -> Signature {
        self.signing.sign(msg)
    }

    /// Sign `msg`, returning the raw 64-byte signature (byte-oriented API so
    /// consumers need not depend on `ed25519-dalek`).
    pub fn sign_bytes(&self, msg: &[u8]) -> [u8; 64] {
        self.sign(msg).to_bytes()
    }

    /// The X25519 secret scalar derived from this identity (clamped).
    pub fn x25519_secret_bytes(&self) -> [u8; 32] {
        x25519_secret_from_seed(&self.seed)
    }

    /// Derive the X25519 public key from an Ed25519 address (Edwards→Montgomery).
    pub fn x25519_public_from_address(address: &PublicKeyBytes) -> CryptoResult<[u8; 32]> {
        let point = CompressedEdwardsY(*address)
            .decompress()
            .ok_or_else(|| CryptoError::InvalidKey("not a curve point".to_string()))?;
        Ok(point.to_montgomery().to_bytes())
    }
}

impl Drop for Identity {
    fn drop(&mut self) {
        self.seed.zeroize();
    }
}

/// Verify an Ed25519 `signature` over `msg` against `public_key`.
pub fn verify_signature(
    public_key: &PublicKeyBytes,
    msg: &[u8],
    signature: &Signature,
) -> CryptoResult<()> {
    let vk =
        VerifyingKey::from_bytes(public_key).map_err(|e| CryptoError::InvalidKey(e.to_string()))?;
    vk.verify(msg, signature)
        .map_err(|_| CryptoError::SignatureInvalid)
}

/// Verify a raw 64-byte Ed25519 signature (byte-oriented; callers need no
/// `ed25519-dalek` types). A wrong-length signature is a verification failure.
pub fn verify_bytes(public_key: &PublicKeyBytes, msg: &[u8], sig: &[u8]) -> CryptoResult<()> {
    let arr: [u8; 64] = sig.try_into().map_err(|_| CryptoError::SignatureInvalid)?;
    verify_signature(public_key, msg, &Signature::from_bytes(&arr))
}

/// X25519 secret = clamp(SHA-512(seed)[..32]) — the Ed25519 signing scalar.
fn x25519_secret_from_seed(seed: &[u8; 32]) -> [u8; 32] {
    let hash = Sha512::digest(seed);
    let mut s = [0u8; 32];
    s.copy_from_slice(&hash[..32]);
    s[0] &= 248;
    s[31] &= 127;
    s[31] |= 64;
    s
}

// ── Argon2 + ChaCha20-Poly1305 keystore ──────────────────────────────

const KEYSTORE_VERSION: u8 = 0x02;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const HEADER_LEN: usize = 1 + SALT_LEN + NONCE_LEN;

/// Passphrase-encrypted identity seed at rest.
///
/// Blob layout: `version(1) ‖ salt(16) ‖ nonce(12) ‖ ChaCha20Poly1305(seed)`.
/// The Argon2-derived key is zeroized after use.
pub struct EncryptedKeystore;

impl EncryptedKeystore {
    /// Seal `identity`'s seed under `passphrase`.
    pub fn seal(identity: &Identity, passphrase: &[u8]) -> CryptoResult<Vec<u8>> {
        let mut salt = [0u8; SALT_LEN];
        OsRng.fill_bytes(&mut salt);
        let mut key = derive_key(passphrase, &salt)?;
        let cipher = ChaCha20Poly1305::new_from_slice(&key)
            .map_err(|e| CryptoError::Keystore(e.to_string()))?;
        key.zeroize();

        let mut nonce_bytes = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce_bytes), identity.seed.as_ref())
            .map_err(|_| CryptoError::SealFailed)?;

        let mut blob = Vec::with_capacity(HEADER_LEN + ciphertext.len());
        blob.push(KEYSTORE_VERSION);
        blob.extend_from_slice(&salt);
        blob.extend_from_slice(&nonce_bytes);
        blob.extend_from_slice(&ciphertext);
        Ok(blob)
    }

    /// Open a sealed keystore `blob` with `passphrase`, recovering the identity.
    pub fn open(blob: &[u8], passphrase: &[u8]) -> CryptoResult<Identity> {
        if blob.len() <= HEADER_LEN || blob[0] != KEYSTORE_VERSION {
            return Err(CryptoError::Keystore("malformed keystore blob".to_string()));
        }
        let salt = &blob[1..1 + SALT_LEN];
        let nonce = &blob[1 + SALT_LEN..HEADER_LEN];
        let ciphertext = &blob[HEADER_LEN..];

        let mut key = derive_key(passphrase, salt)?;
        let cipher = ChaCha20Poly1305::new_from_slice(&key)
            .map_err(|e| CryptoError::Keystore(e.to_string()))?;
        key.zeroize();

        let mut plaintext = cipher
            .decrypt(Nonce::from_slice(nonce), ciphertext)
            .map_err(|_| CryptoError::SealFailed)?;
        let seed: [u8; 32] = plaintext
            .as_slice()
            .try_into()
            .map_err(|_| CryptoError::Keystore("decrypted seed length".to_string()))?;
        plaintext.zeroize();
        Ok(Identity::from_seed(seed))
    }
}

fn derive_key(passphrase: &[u8], salt: &[u8]) -> CryptoResult<[u8; 32]> {
    let mut key = [0u8; 32];
    Argon2::default()
        .hash_password_into(passphrase, salt, &mut key)
        .map_err(|e| CryptoError::Keystore(e.to_string()))?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blake3_and_sha256_known_vectors() {
        // SHA-256("abc") — FIPS 180-4 KAT.
        assert_eq!(
            hex::encode(sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // BLAKE3("") — official test vector.
        assert_eq!(
            hex::encode(blake3_hash(b"")),
            "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
        );
    }

    #[test]
    fn sign_verify_roundtrip() -> CryptoResult<()> {
        let id = Identity::generate();
        let sig = id.sign(b"sovereign payload");
        verify_signature(&id.public_key(), b"sovereign payload", &sig)?;
        assert!(verify_signature(&id.public_key(), b"tampered", &sig).is_err());
        Ok(())
    }

    #[test]
    fn derived_x25519_keypair_is_consistent() -> CryptoResult<()> {
        use crypto_box::SecretKey;
        let id = Identity::generate();
        let secret = SecretKey::from(id.x25519_secret_bytes());
        let from_secret = *secret.public_key().as_bytes();
        let from_address = Identity::x25519_public_from_address(&id.public_key())?;
        assert_eq!(from_secret, from_address);
        Ok(())
    }

    #[test]
    fn keystore_roundtrip_and_wrong_passphrase() -> CryptoResult<()> {
        let id = Identity::generate();
        let pubkey = id.public_key();
        let blob = EncryptedKeystore::seal(&id, b"correct horse")?;
        // Right passphrase recovers the same identity.
        let recovered = EncryptedKeystore::open(&blob, b"correct horse")?;
        assert_eq!(recovered.public_key(), pubkey);
        // Wrong passphrase fails (AEAD auth), never panics.
        assert!(EncryptedKeystore::open(&blob, b"battery staple").is_err());
        // Malformed blob is rejected.
        assert!(EncryptedKeystore::open(b"\x02short", b"x").is_err());
        Ok(())
    }
}
