//! `aead` — the SDBP confidentiality primitive.
//!
//! Authenticated encryption over an X25519 Diffie-Hellman shared secret, using
//! **XChaCha20-Poly1305** (the extended-nonce ChaCha20-Poly1305 AEAD, via
//! [`crypto_box::ChaChaBox`]). This is the same backend the top-down pillars'
//! sealed-message paths already prove against.
//!
//! **Byte-oriented**: callers pass raw X25519 key bytes (`[u8; 32]`) — the same
//! ones [`crate::Identity::x25519_secret_bytes`] /
//! [`crate::Identity::x25519_public_from_address`] produce — so consumers like
//! `iedb-transport` need not depend on `crypto_box` themselves.
//!
//! **Layering note**: this exposes the *primitive* only. The `SdbpEnvelope` that
//! the transport layer seals lives in `iedb-transport` (which depends on this
//! crate); returning an `SdbpEnvelope` from here would be a dependency cycle, so
//! envelope construction stays in transport and composes these functions.
//!
//! Static-static DH: both parties use their stable identity keys. There is no
//! forward secrecy (an ephemeral-static variant is a future refinement); a
//! compromised long-term key exposes past traffic. Every fallible path returns
//! [`CryptoError`] — wrong key, tamper, and truncation all map to
//! [`CryptoError::SealFailed`] with no decryption oracle. Zero panics.

use crate::crypto::{CryptoError, CryptoResult};
use crypto_box::aead::{Aead, AeadCore};
use crypto_box::{ChaChaBox, Nonce, PublicKey, SecretKey};
use rand::rngs::OsRng;

/// XChaCha20-Poly1305 nonce width (extended nonce).
const NONCE_LEN: usize = 24;

/// Seal `plaintext` from `sender_secret` (X25519 secret bytes) to
/// `receiver_public` (X25519 public bytes). The wire layout is
/// `nonce(24) ‖ ciphertext‖tag`; the random nonce makes each sealing distinct.
pub fn seal(
    sender_secret: &[u8; 32],
    receiver_public: &[u8; 32],
    plaintext: &[u8],
) -> CryptoResult<Vec<u8>> {
    let boxed = ChaChaBox::new(&PublicKey::from(*receiver_public), &SecretKey::from(*sender_secret));
    let nonce = ChaChaBox::generate_nonce(&mut OsRng);
    let ciphertext = boxed.encrypt(&nonce, plaintext).map_err(|_| CryptoError::SealFailed)?;

    let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    out.extend_from_slice(nonce.as_slice());
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Open a box sealed by `sender_public` for `receiver_secret`. The DH shared
/// secret is symmetric, so `(receiver_secret, sender_public)` here recovers what
/// `(sender_secret, receiver_public)` sealed. Tamper, wrong key, or a truncated
/// blob all fail as [`CryptoError::SealFailed`].
pub fn open(
    receiver_secret: &[u8; 32],
    sender_public: &[u8; 32],
    sealed: &[u8],
) -> CryptoResult<Vec<u8>> {
    if sealed.len() < NONCE_LEN {
        return Err(CryptoError::SealFailed);
    }
    let (nonce_bytes, ciphertext) = sealed.split_at(NONCE_LEN);
    let boxed = ChaChaBox::new(&PublicKey::from(*sender_public), &SecretKey::from(*receiver_secret));
    boxed
        .decrypt(Nonce::from_slice(nonce_bytes), ciphertext)
        .map_err(|_| CryptoError::SealFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Identity;

    /// Two sovereign identities can seal/open to each other (static-static DH).
    #[test]
    fn seal_open_roundtrip() -> CryptoResult<()> {
        let alice = Identity::generate();
        let bob = Identity::generate();
        let bob_x_pub = Identity::x25519_public_from_address(&bob.public_key())?;
        let alice_x_pub = Identity::x25519_public_from_address(&alice.public_key())?;

        let sealed = seal(&alice.x25519_secret_bytes(), &bob_x_pub, b"sovereign secret")?;
        // The ciphertext must not contain the plaintext.
        assert!(!sealed.windows(16).any(|w| w == b"sovereign secret"));

        let opened = open(&bob.x25519_secret_bytes(), &alice_x_pub, &sealed)?;
        assert_eq!(opened, b"sovereign secret");
        Ok(())
    }

    /// A tampered ciphertext fails the Poly1305 tag — never returns plaintext.
    #[test]
    fn tamper_is_rejected() -> CryptoResult<()> {
        let alice = Identity::generate();
        let bob = Identity::generate();
        let bob_x_pub = Identity::x25519_public_from_address(&bob.public_key())?;
        let alice_x_pub = Identity::x25519_public_from_address(&alice.public_key())?;

        let mut sealed = seal(&alice.x25519_secret_bytes(), &bob_x_pub, b"do not flip")?;
        let last = sealed.len() - 1;
        sealed[last] ^= 0x01;
        assert!(open(&bob.x25519_secret_bytes(), &alice_x_pub, &sealed).is_err());
        Ok(())
    }

    /// The wrong recipient key cannot open the box.
    #[test]
    fn wrong_key_is_rejected() -> CryptoResult<()> {
        let alice = Identity::generate();
        let bob = Identity::generate();
        let mallory = Identity::generate();
        let bob_x_pub = Identity::x25519_public_from_address(&bob.public_key())?;
        let alice_x_pub = Identity::x25519_public_from_address(&alice.public_key())?;

        let sealed = seal(&alice.x25519_secret_bytes(), &bob_x_pub, b"for bob only")?;
        assert!(open(&mallory.x25519_secret_bytes(), &alice_x_pub, &sealed).is_err());
        Ok(())
    }

    /// A truncated blob (shorter than the nonce) is rejected without panic.
    #[test]
    fn truncated_blob_is_rejected() {
        let bob = Identity::generate();
        let dummy = [7u8; 32];
        assert!(open(&bob.x25519_secret_bytes(), &dummy, b"short").is_err());
    }
}
