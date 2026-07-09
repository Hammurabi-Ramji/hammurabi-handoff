//! # Hammurabi Handoff — unified Agent Service Provider (ASP)
//!
//! Two architectural layers fused into one local-first, zero-telemetry app
//! ("Legible Autonomy"):
//!
//! 1. **RAMesh (Readable Agent Mesh)** — an IRC-style substrate where every
//!    agent intent, gate decision, and vault action is a human-readable chat
//!    line the operator can watch ([`mesh`], [`intent`]).
//! 2. **Hammurabi Handshake** — a JIT credential vault: high-privilege agent
//!    requests are intercepted by an x402 payment gate ([`x402`]); once the
//!    micro-payment clears, the vault mints a payload-bound, ephemeral
//!    Ed25519 credential with a 60-second TTL ([`vault`]); the signed intent
//!    is then packaged for gasless 1Shot → Arbitrum Stylus settlement
//!    ([`settlement`] — formatting only, no network).
//!
//! ## Sovereignty posture (BSM-C)
//! - No cloud persistence: keys are ephemeral, minted in-process, and the
//!   seed is zeroized on drop (`iedb-crypto::Identity`). Nothing is written
//!   to disk.
//! - No telemetry: the only observer is the local RAMesh feed.
//! - One enforcement point: the Tauri IPC commands drive requests through
//!   the *same* Axum router (in-process `tower::oneshot`) that loopback
//!   HTTP clients hit — there is no code path around the x402 gate.
//! - Loopback only: [`server::serve_loopback`] binds `127.0.0.1` and
//!   nothing else.
//! - Caller-supplied time on every cryptographic path (mirrors the
//!   workspace `jcs` discipline); the ambient clock is used only for chat
//!   timestamps and the HTTP edge.

#![forbid(unsafe_code)]

pub mod intent;
pub mod mesh;
pub mod okx_bridge;
pub mod server;
pub mod settlement;
pub mod settlement_backend;
pub mod vault;
pub mod x402;

// ── Tauri desktop shell (only under `tauri-app`) ─────────────────────
#[cfg(feature = "tauri-app")]
pub mod app;

#[cfg(feature = "tauri-app")]
pub mod commands;
