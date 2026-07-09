//! Typed OKX.AI ASP manifest — a compiler-checked mirror of the hand-written
//! `desktop/hammurabi-handoff/asp/manifest.json`.
//!
//! The submission ships JSON, but modeling it here means the schema is
//! validated at build time and the two can be kept in lockstep by a test.

use serde::{Deserialize, Serialize};

use crate::{OkaError, Result};

/// Manifest author block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Author {
    pub name: String,
    pub url: String,
}

/// One advertised capability (endpoint + description). Kept intentionally
/// loose (free-form `serde_json::Value` for schemas) so it round-trips any
/// capability shape without over-constraining the marketplace contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capability {
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty", flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Sovereignty posture — the verifiable local-first / zero-telemetry claims.
/// Modeled explicitly (not a loose `Value`) so the typed manifest can never
/// silently drop these guarantees on a round-trip. See `SUBMISSION_LEDGER.md`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sovereignty {
    pub binding: String,
    pub telemetry: String,
    pub key_persistence: String,
    pub clock_discipline: String,
}

/// Marketplace display block (logo path + brand colors).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Display {
    pub logo: String,
    pub primary_color: String,
    pub accent_color: String,
}

/// x402 pricing block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pricing {
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_mint_fee_micro: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_mint_fee_display: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_verify_fee: Option<String>,
}

/// The OKX.AI Agent Service Provider manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AspManifest {
    pub manifest_version: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: Author,
    pub capabilities: serde_json::Map<String, serde_json::Value>,
    pub pricing: Pricing,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub integrations: serde_json::Map<String, serde_json::Value>,
    /// Sovereignty posture. Optional in the schema, but present and enforced
    /// for this build — the whole premise depends on these claims surviving.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sovereignty: Option<Sovereignty>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<Display>,
}

impl AspManifest {
    /// Parse a manifest from JSON text, mapping failures to [`OkaError::Schema`].
    pub fn from_json(text: &str) -> Result<Self> {
        serde_json::from_str(text).map_err(|e| OkaError::Schema(e.to_string()))
    }

    /// Serialize back to pretty JSON.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(|e| OkaError::Schema(e.to_string()))
    }

    /// Minimal validity: non-empty identity, x402 pricing, at least one capability.
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            return Err(OkaError::Schema("manifest name is empty".into()));
        }
        if self.pricing.model != "x402" {
            return Err(OkaError::Schema(format!(
                "expected x402 pricing model, got {:?}",
                self.pricing.model
            )));
        }
        if self.capabilities.is_empty() {
            return Err(OkaError::Schema(
                "manifest advertises no capabilities".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> &'static str {
        r#"{
            "manifest_version": "1.0",
            "name": "Hammurabi Handoff — JIT Vault",
            "description": "JIT credential vault ASP",
            "version": "0.1.0",
            "author": { "name": "HCC", "url": "https://example.invalid" },
            "capabilities": {
                "credential_minting": { "description": "mint", "endpoint": "POST /execute" }
            },
            "pricing": { "model": "x402", "scheme": "exact", "asset": "USDC", "credential_mint_fee_micro": 500000 },
            "integrations": { "x402": "live" }
        }"#
    }

    #[test]
    fn round_trips_and_validates() {
        let m = AspManifest::from_json(sample()).expect("parse");
        m.validate().expect("valid");
        let back = AspManifest::from_json(&m.to_json().expect("ser")).expect("reparse");
        assert_eq!(m, back);
        assert_eq!(m.pricing.credential_mint_fee_micro, Some(500_000));
    }

    #[test]
    fn rejects_non_x402_pricing() {
        let bad = sample().replace("\"x402\"", "\"stripe\"");
        let m = AspManifest::from_json(&bad).expect("parse");
        assert!(m.validate().is_err());
    }

    /// The real shipped manifest. Path is relative to this source file.
    const SHIPPED_MANIFEST: &str =
        include_str!("../../../desktop/hammurabi-handoff/asp/manifest.json");

    /// Lockstep invariant: the typed model must be a *lossless* mirror of the
    /// submitted `asp/manifest.json`. Parsing then re-serializing must yield a
    /// byte-for-semantics-identical document — no field may be silently
    /// dropped. This is the code mathematically backing `SUBMISSION_LEDGER.md`.
    #[test]
    fn shipped_manifest_round_trips_losslessly() {
        let original: serde_json::Value =
            serde_json::from_str(SHIPPED_MANIFEST).expect("shipped manifest is valid JSON");

        let typed = AspManifest::from_json(SHIPPED_MANIFEST).expect("parses into typed model");
        typed.validate().expect("shipped manifest validates");

        let reserialized: serde_json::Value =
            serde_json::from_str(&typed.to_json().expect("serialize")).expect("reparse");

        assert_eq!(
            original, reserialized,
            "typed AspManifest dropped or altered a field from the shipped manifest.json"
        );
    }

    /// The sovereignty claims — the architectural high ground — must be
    /// present and non-empty in the shipped manifest.
    #[test]
    fn shipped_manifest_preserves_sovereignty() {
        let typed = AspManifest::from_json(SHIPPED_MANIFEST).expect("parse");
        let s = typed.sovereignty.expect("sovereignty block present");
        assert_eq!(s.telemetry.to_lowercase(), "none");
        assert!(s.binding.contains("127.0.0.1"));
        assert!(!s.key_persistence.trim().is_empty());
        assert!(!s.clock_discipline.trim().is_empty());
    }

    #[test]
    fn rejects_empty_capabilities() {
        let bad = sample().replace(
            "\"credential_minting\": { \"description\": \"mint\", \"endpoint\": \"POST /execute\" }",
            "",
        );
        let m = AspManifest::from_json(&bad).expect("parse");
        assert!(m.validate().is_err());
    }
}
