//! # iedb-oka — IEDB ↔ OKX.AI integration boundary
//!
//! The adapter seam between the OKX.AI Genesis / HackQuest distribution layer
//! and the sovereign Hammurabi Handoff pipeline (`desktop/hammurabi-handoff`).
//!
//! ## Scope (tight, hackathon-bounded)
//! This crate owns exactly the OKX-facing edge and nothing deeper:
//! 1. **ASP manifest modeling** ([`manifest`]) — typed, serde-round-tripping
//!    representation of the OKX.AI Agent Service Provider manifest, so the
//!    hand-written `asp/manifest.json` has a compiler-checked schema.
//! 2. **OKX data-structure interop** ([`interop`]) — the request/response
//!    shapes the OKX marketplace speaks, mapped to/from the internal
//!    `AgentIntent` / settlement-envelope vocabulary.
//! 3. **Live boundary** ([`client`], `okx-live` feature) — the *only* place
//!    network code is allowed; compiled out by default.
//!
//! ## What it deliberately does NOT do
//! No credential minting, no payment gating, no signing — those belong to the
//! Handoff vault/x402 layers. This crate is a translator, not an authority:
//! it never holds keys and never mutates sovereign state. Keeping the OKX
//! edge isolated here means the marketplace's schema churn can't reach into
//! the vault.

#![forbid(unsafe_code)]

pub mod interop;
pub mod manifest;

#[cfg(feature = "okx-live")]
pub mod client;

/// Adapter-boundary error. Marketplace-facing failures never surface as
/// panics into the sovereign pipeline — they resolve to one of these.
#[derive(Debug, thiserror::Error)]
pub enum OkaError {
    /// A manifest or interop payload failed (de)serialization / validation.
    #[error("schema error: {0}")]
    Schema(String),
    /// The live OKX boundary failed (only reachable under `okx-live`).
    #[error("okx boundary error: {0}")]
    Boundary(String),
}

/// Adapter result alias.
pub type Result<T> = std::result::Result<T, OkaError>;
