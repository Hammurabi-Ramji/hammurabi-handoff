//! JCS — JSON Canonicalization Scheme (RFC 8785), constrained subset.
//!
//! The Hammurabi Handshake signs the JSON-RPC `params` object that crosses the
//! local IPC bus. A signature over a `serde_json::Value` is only verifiable if
//! both ends serialize that value to **byte-identical** form — otherwise any
//! re-parse/re-emit in transit (which JSON-RPC routing does) yields phantom
//! verification failures. RFC 8785 defines that canonical form.
//!
//! ## Why this is a *constrained* implementation — and why that is the honest one
//!
//! Full RFC 8785's single hard clause is **number serialization**: it mandates
//! the ECMAScript `Number::toString` shortest round-tripping representation of
//! an IEEE-754 double (the Ryū/Grisu shortest-float problem). Hand-rolling that
//! correctly is research-grade, and `serde_json` does not emit it. This protocol
//! carries **no floats** — every numeric is a bounded non-negative integer
//! (`max_agents`). So instead of risking a fragile float formatter, we implement
//! the provably-sound subset and **reject** anything outside it:
//!
//! - object keys sorted by **UTF-16 code units** (RFC 8785 §3.2.3);
//! - strings escaped per RFC 8785 §3.2.2.2 (quote, backslash, control chars
//!   only; `/` left literal; all other code points emitted as raw UTF-8);
//! - numbers accepted **only** when exactly representable as `i64`/`u64`;
//! - **any** float, `NaN`, or `Infinity` is a hard [`CryptoError::Canonicalization`].
//!
//! We never sign bytes we cannot deterministically regenerate. If float payloads
//! ever become real, that — not before — is the moment to adopt a vetted full
//! RFC 8785 crate. See `docs/specs/sovereign-physics.md` §7.

use crate::crypto::{verify_bytes, CryptoError, CryptoResult, Identity, PublicKeyBytes};
use serde_json::Value;

/// Canonicalize a [`Value`] to its RFC 8785 byte form (constrained subset).
///
/// Returns [`CryptoError::Canonicalization`] for any non-integer number — the
/// fail-closed boundary that keeps signing deterministic.
pub fn canonicalize(value: &Value) -> CryptoResult<Vec<u8>> {
    let mut out = String::new();
    write_value(value, &mut out)?;
    Ok(out.into_bytes())
}

fn write_value(value: &Value, out: &mut String) -> CryptoResult<()> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(n) => write_number(n, out)?,
        Value::String(s) => write_string(s, out),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(item, out)?;
            }
            out.push(']');
        }
        Value::Object(map) => {
            // RFC 8785 §3.2.3: members sorted by UTF-16 code units of the key.
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|a, b| utf16_cmp(a, b));
            out.push('{');
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(key, out);
                out.push(':');
                // Key came from `map`, so the lookup cannot miss.
                if let Some(v) = map.get(*key) {
                    write_value(v, out)?;
                }
            }
            out.push('}');
        }
    }
    Ok(())
}

/// Constrained number rule: integers only. Floats are refused, not approximated.
fn write_number(n: &serde_json::Number, out: &mut String) -> CryptoResult<()> {
    if let Some(u) = n.as_u64() {
        out.push_str(&u.to_string());
        Ok(())
    } else if let Some(i) = n.as_i64() {
        out.push_str(&i.to_string());
        Ok(())
    } else {
        // is_f64 (or non-finite): outside the sound subset — fail closed.
        Err(CryptoError::Canonicalization(format!(
            "non-integer number `{n}` is not canonicalizable in the integer-only \
             signing subset; floats are refused by construction"
        )))
    }
}

/// RFC 8785 §3.2.2.2 string production: minimal escaping, raw UTF-8 otherwise.
fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // Remaining C0 control chars escape to lowercase \u00xx.
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            // Everything else, including non-ASCII, is emitted literally (UTF-8).
            // Note: `/` is deliberately NOT escaped, per the spec.
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Compare two strings by their UTF-16 code-unit sequences. Equals code-point
/// order for the BMP; differs only for non-BMP keys (surrogate pairs), which the
/// spec still requires be ordered by UTF-16 units.
fn utf16_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

// ── Canonical signing seam (the bytes that reach the Ed25519 verifier) ───────
//
// The Handshake binds the canonical params to a caller-supplied `timestamp`
// (replay window), mirroring `handshake::signer`'s `body‖0‖timestamp_le`
// construction. The `0x00` separator is unambiguous because canonical JSON
// never contains a NUL byte. Time is caller-supplied — never an ambient clock.

/// Build the exact message bytes signed/verified for a payload:
/// `canonical(params) ‖ 0x00 ‖ timestamp_le`.
pub fn canonical_message(params: &Value, timestamp: i64) -> CryptoResult<Vec<u8>> {
    let mut msg = canonicalize(params)?;
    msg.push(0);
    msg.extend_from_slice(&timestamp.to_le_bytes());
    Ok(msg)
}

/// Sign a payload's canonical form bound to `timestamp`. Returns the 64-byte
/// Ed25519 signature, or a canonicalization rejection (e.g. a float in params).
pub fn sign_payload(identity: &Identity, params: &Value, timestamp: i64) -> CryptoResult<[u8; 64]> {
    let msg = canonical_message(params, timestamp)?;
    Ok(identity.sign_bytes(&msg))
}

/// Verify a payload signature against its canonical form + timestamp. Fails with
/// [`CryptoError::SignatureInvalid`] on mismatch, or `Canonicalization` if the
/// params cannot be canonicalized (which is itself a rejection — unverifiable).
pub fn verify_payload(
    public_key: &PublicKeyBytes,
    params: &Value,
    timestamp: i64,
    signature: &[u8; 64],
) -> CryptoResult<()> {
    let msg = canonical_message(params, timestamp)?;
    verify_bytes(public_key, &msg, signature)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn canon(v: &Value) -> String {
        String::from_utf8(canonicalize(v).expect("should canonicalize")).expect("utf8")
    }

    #[test]
    fn sorts_object_keys_and_strips_whitespace() {
        let v = json!({ "b": 1, "a": 2, "c": 3 });
        assert_eq!(canon(&v), r#"{"a":2,"b":1,"c":3}"#);
    }

    #[test]
    fn nested_structure_is_deterministic() {
        let v = json!({
            "params": { "max_agents": 3, "task_id": "tsk_8f9a2b" },
            "method": "horde.execute_bsmc_task"
        });
        assert_eq!(
            canon(&v),
            r#"{"method":"horde.execute_bsmc_task","params":{"max_agents":3,"task_id":"tsk_8f9a2b"}}"#
        );
    }

    #[test]
    fn two_orderings_canonicalize_identically() {
        // The footgun this closes: same logical payload, different key order.
        let a = json!({ "x": 1, "y": [2, 3], "z": "q" });
        let b = json!({ "z": "q", "y": [2, 3], "x": 1 });
        assert_eq!(canon(&a), canon(&b));
    }

    #[test]
    fn strings_escape_per_spec() {
        let v = json!("a\"b\\c\nd\te/f");
        // `/` stays literal; quote, backslash, newline, tab escape.
        assert_eq!(canon(&v), r#""a\"b\\c\nd\te/f""#);
    }

    #[test]
    fn control_char_uses_lowercase_u_escape() {
        let v = Value::String("\u{0001}".to_string());
        assert_eq!(canon(&v), "\"\\u0001\"");
    }

    #[test]
    fn integers_pass_through() {
        assert_eq!(canon(&json!(0)), "0");
        assert_eq!(canon(&json!(-7)), "-7");
        assert_eq!(canon(&json!(u64::MAX)), u64::MAX.to_string());
    }

    #[test]
    fn floats_are_rejected_not_approximated() {
        // 9.5 is arbitrary test data — any float must be rejected, this
        // isn't standing in for a math constant (avoids a false-positive
        // clippy::approx_constant trip on a value like 3.14).
        let err = canonicalize(&json!(9.5)).unwrap_err();
        assert!(matches!(err, CryptoError::Canonicalization(_)));
    }

    #[test]
    fn float_nested_in_object_is_rejected() {
        let v = json!({ "ok": 1, "bad": 2.5 });
        assert!(canonicalize(&v).is_err());
    }

    #[test]
    fn sign_then_verify_roundtrips() {
        let id = Identity::generate();
        let params = json!({ "task_id": "tsk_1", "intent": "x", "max_agents": 3 });
        let sig = sign_payload(&id, &params, 1000).expect("sign");
        assert!(verify_payload(&id.public_key(), &params, 1000, &sig).is_ok());
    }

    #[test]
    fn verify_survives_key_reordering() {
        // THE footgun: the wire payload re-emits params with different key order.
        // Canonicalization must make the signature order-independent.
        let id = Identity::generate();
        let signed = json!({ "a": 1, "b": "two", "c": [3, 4] });
        let sig = sign_payload(&id, &signed, 42).expect("sign");
        let reordered = json!({ "c": [3, 4], "a": 1, "b": "two" });
        assert!(verify_payload(&id.public_key(), &reordered, 42, &sig).is_ok());
    }

    #[test]
    fn verify_rejects_tampered_params() {
        let id = Identity::generate();
        let params = json!({ "max_agents": 3 });
        let sig = sign_payload(&id, &params, 1000).expect("sign");
        let tampered = json!({ "max_agents": 9 });
        assert!(verify_payload(&id.public_key(), &tampered, 1000, &sig).is_err());
    }

    #[test]
    fn verify_rejects_timestamp_replay_shift() {
        let id = Identity::generate();
        let params = json!({ "max_agents": 3 });
        let sig = sign_payload(&id, &params, 1000).expect("sign");
        // Same payload, different timestamp → different message → invalid.
        assert!(verify_payload(&id.public_key(), &params, 1001, &sig).is_err());
    }

    #[test]
    fn signing_a_float_payload_is_refused() {
        let id = Identity::generate();
        let params = json!({ "ratio": 0.5 });
        assert!(matches!(
            sign_payload(&id, &params, 1000),
            Err(CryptoError::Canonicalization(_))
        ));
    }

    #[test]
    fn non_bmp_keys_order_by_utf16_units() {
        // U+10000 (non-BMP) vs U+FFFF (BMP). By UTF-16 units the surrogate lead
        // (0xD800) sorts before 0xFFFF, so the non-BMP key serializes first.
        let v = json!({ "\u{FFFF}": 1, "\u{10000}": 2 });
        let out = canon(&v);
        let pos_non_bmp = out.find('\u{10000}').expect("non-bmp key present");
        let pos_bmp = out.find('\u{FFFF}').expect("bmp key present");
        assert!(pos_non_bmp < pos_bmp, "non-BMP key must sort first: {out}");
    }
}
