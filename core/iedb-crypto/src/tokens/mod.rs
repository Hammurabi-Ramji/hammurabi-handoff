//! IEDB Tokens — Good Token economics (purified bedrock module).
//!
//! Good Tokens are the economic unit of the Sovereign Appliance Suite: minted
//! from batches of verified IEDB events, carrying a BLAKE3 proof chain, and
//! ranked by the Proof-of-Merit Oracle.
//!
//! **Purification (Item 3 — quarantine liquidation).** The legacy form was a
//! SQLite-backed CRUD layer (`sqlx::SqlitePool`, schema from the non-workspace
//! `iedb-store` crate) that read the wall clock in five places (`Utc::now` +
//! `datetime('now')` inside SQL). That violated the bedrock's invariants. This
//! module is now:
//! - **Zero-IO** — an in-memory [`TokenLedger`] over a `BTreeMap`; persistence
//!   (e.g. to the bespoke `SovereignLedger` WAL) is the caller's concern.
//! - **Deterministic** — token ids are content-addressed via BLAKE3, not
//!   time/random `ulid`s; all timestamps are **caller-supplied** (no ambient
//!   clock), so a fixed input reproduces a bit-identical token.
//! - **Typed** — every fallible path returns the bedrock's [`CryptoError`];
//!   zero `unwrap`/`expect`/`panic`, zero `anyhow`.
//!
//! Lifecycle is a strict state machine: `Minted → Active → Spent`, with `Burned`
//! reachable from `Minted`/`Active` and `Revoke` an administrative override from
//! any non-revoked state. Invalid transitions are typed errors (the legacy SQL
//! silently no-op'd them).

use crate::crypto::{CryptoError, CryptoResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Version of the cryptographic bedrock crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Status of a Good Token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenStatus {
    Minted,
    Active,
    Spent,
    Burned,
    Revoked,
}

impl std::fmt::Display for TokenStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            TokenStatus::Minted => "minted",
            TokenStatus::Active => "active",
            TokenStatus::Spent => "spent",
            TokenStatus::Burned => "burned",
            TokenStatus::Revoked => "revoked",
        };
        f.write_str(s)
    }
}

/// A Good Token — the economic unit of the Sovereign Appliance Suite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoodToken {
    pub token_id: String,
    pub batch_id: i64,
    pub event_count: i64,
    pub first_timestamp: String,
    pub last_timestamp: String,
    pub chained_root: String,
    pub proof_uri: Option<String>,
    pub status: TokenStatus,
    /// Caller-supplied mint time (e.g. RFC3339) — never an ambient clock.
    pub minted_at: String,
    pub merit_score: f64,
    pub metadata: serde_json::Value,
}

/// A merit-ranked token result from the Proof of Merit Oracle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeritRanking {
    pub token_id: String,
    pub merit_score: f64,
    pub event_count: i64,
    pub chained_root: String,
    pub rank: usize,
}

// ─── Pure, deterministic primitives ───────────────────────────────────

/// The chained root: a BLAKE3 hash over all event CIDs in order, forming a
/// Merkle-like commitment to the batch. Order-sensitive and deterministic.
pub fn chained_root(event_cids: &[String]) -> String {
    let mut hasher = blake3::Hasher::new();
    for cid in event_cids {
        hasher.update(cid.as_bytes());
        hasher.update(b"\n");
    }
    hex::encode(hasher.finalize().as_bytes())
}

/// Content-addressed token id, derived deterministically from the batch and its
/// chained root (replaces the time/random `ulid` of the legacy form).
fn derive_token_id(batch_id: i64, chained_root: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&batch_id.to_le_bytes());
    hasher.update(chained_root.as_bytes());
    format!("GT-{}", hex::encode(&hasher.finalize().as_bytes()[..16]))
}

/// Compute the merit score for a token.
///
/// Formula: `event_count * ln(event_count + 1) * verification_weight`. Rewards
/// tokens aggregating more verified events, with logarithmic saturation to
/// prevent runaway scores. `verification_weight` can be raised for tokens
/// anchored to an external chain.
pub fn compute_merit_score(event_count: i64, verification_weight: f64) -> f64 {
    let n = event_count as f64;
    n * (n + 1.0).ln() * verification_weight
}

/// Rank tokens by merit score (descending), assigning 1-based ranks.
pub fn rank_by_merit(tokens: &[GoodToken]) -> Vec<MeritRanking> {
    let mut sorted: Vec<&GoodToken> = tokens.iter().collect();
    sorted.sort_by(|a, b| {
        b.merit_score
            .partial_cmp(&a.merit_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    sorted
        .iter()
        .enumerate()
        .map(|(i, t)| MeritRanking {
            token_id: t.token_id.clone(),
            merit_score: t.merit_score,
            event_count: t.event_count,
            chained_root: t.chained_root.clone(),
            rank: i + 1,
        })
        .collect()
}

// ─── In-memory token ledger ───────────────────────────────────────────

/// A zero-IO, deterministic ledger of Good Tokens keyed by token id.
///
/// `BTreeMap` gives deterministic iteration order (by id), so listings and
/// counts are reproducible. Persistence is delegated to the caller — serialize
/// any [`GoodToken`] and commit it to the `SovereignLedger`.
#[derive(Debug, Default, Clone)]
pub struct TokenLedger {
    tokens: BTreeMap<String, GoodToken>,
}

impl TokenLedger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mint a Good Token from a batch of verified event CIDs. The id is
    /// content-addressed; `minted_at`/`first_timestamp`/`last_timestamp` are
    /// caller-supplied. Re-minting the same batch (identical id) is rejected.
    pub fn mint(
        &mut self,
        batch_id: i64,
        event_cids: &[String],
        first_timestamp: &str,
        last_timestamp: &str,
        minted_at: &str,
        metadata: serde_json::Value,
    ) -> CryptoResult<GoodToken> {
        let root = chained_root(event_cids);
        let token_id = derive_token_id(batch_id, &root);
        if self.tokens.contains_key(&token_id) {
            return Err(CryptoError::Token(format!(
                "token already minted: {token_id}"
            )));
        }
        let event_count = event_cids.len() as i64;
        let token = GoodToken {
            token_id: token_id.clone(),
            batch_id,
            event_count,
            first_timestamp: first_timestamp.to_string(),
            last_timestamp: last_timestamp.to_string(),
            chained_root: root,
            proof_uri: None,
            status: TokenStatus::Minted,
            minted_at: minted_at.to_string(),
            merit_score: compute_merit_score(event_count, 1.0),
            metadata,
        };
        self.tokens.insert(token_id, token.clone());
        Ok(token)
    }

    /// Borrow a token by id.
    pub fn get(&self, token_id: &str) -> CryptoResult<&GoodToken> {
        self.tokens
            .get(token_id)
            .ok_or_else(|| CryptoError::Token(format!("token not found: {token_id}")))
    }

    /// Enforce a status transition, rejecting moves not allowed from the
    /// token's current state.
    fn transition(
        &mut self,
        token_id: &str,
        allowed_from: &[TokenStatus],
        to: TokenStatus,
    ) -> CryptoResult<()> {
        let token = self
            .tokens
            .get_mut(token_id)
            .ok_or_else(|| CryptoError::Token(format!("token not found: {token_id}")))?;
        if !allowed_from.contains(&token.status) {
            return Err(CryptoError::Token(format!(
                "invalid transition for {token_id}: {} → {to}",
                token.status
            )));
        }
        token.status = to;
        Ok(())
    }

    /// Activate a minted token (`Minted → Active`).
    pub fn activate(&mut self, token_id: &str) -> CryptoResult<()> {
        self.transition(token_id, &[TokenStatus::Minted], TokenStatus::Active)
    }

    /// Spend an active token (`Active → Spent`).
    pub fn spend(&mut self, token_id: &str) -> CryptoResult<()> {
        self.transition(token_id, &[TokenStatus::Active], TokenStatus::Spent)
    }

    /// Burn a token, removing it from circulation (`Minted`/`Active → Burned`).
    pub fn burn(&mut self, token_id: &str, _reason: &str) -> CryptoResult<()> {
        self.transition(
            token_id,
            &[TokenStatus::Minted, TokenStatus::Active],
            TokenStatus::Burned,
        )
    }

    /// Administratively revoke a token from any non-revoked state.
    pub fn revoke(&mut self, token_id: &str, _reason: &str) -> CryptoResult<()> {
        self.transition(
            token_id,
            &[
                TokenStatus::Minted,
                TokenStatus::Active,
                TokenStatus::Spent,
                TokenStatus::Burned,
            ],
            TokenStatus::Revoked,
        )
    }

    /// List minted/active tokens, most-meritorious first, capped at `limit`.
    pub fn list_active(&self, limit: usize) -> Vec<GoodToken> {
        let active: Vec<GoodToken> = self
            .tokens
            .values()
            .filter(|t| matches!(t.status, TokenStatus::Minted | TokenStatus::Active))
            .cloned()
            .collect();
        let ranked = rank_by_merit(&active);
        ranked
            .iter()
            .take(limit)
            .filter_map(|r| self.tokens.get(&r.token_id).cloned())
            .collect()
    }

    /// The top `limit` tokens by merit (over the active set).
    pub fn top_by_merit(&self, limit: usize) -> Vec<MeritRanking> {
        let active = self.list_active(usize::MAX);
        let mut rankings = rank_by_merit(&active);
        rankings.truncate(limit);
        rankings
    }

    /// Count tokens currently in `status`.
    pub fn count_by_status(&self, status: TokenStatus) -> usize {
        self.tokens.values().filter(|t| t.status == status).count()
    }

    /// Total number of tokens held.
    pub fn total(&self) -> usize {
        self.tokens.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: &str = "2026-06-01T00:00:00Z";
    const T1: &str = "2026-06-01T01:00:00Z";

    fn cids(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("cid-{i}")).collect()
    }

    #[test]
    fn merit_score_monotonic_and_zero_floor() {
        assert_eq!(compute_merit_score(0, 1.0), 0.0);
        let (s1, s2, s3) = (
            compute_merit_score(10, 1.0),
            compute_merit_score(100, 1.0),
            compute_merit_score(1000, 1.0),
        );
        assert!(s1 > 0.0 && s2 > s1 && s3 > s2);
        // verification_weight scales linearly.
        assert!((compute_merit_score(100, 2.0) - 2.0 * s2).abs() < 1e-9);
    }

    #[test]
    fn chained_root_is_deterministic_and_order_sensitive() {
        let a = chained_root(&["x".into(), "y".into()]);
        let b = chained_root(&["x".into(), "y".into()]);
        let c = chained_root(&["y".into(), "x".into()]);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn mint_is_deterministic_and_id_content_addressed() -> CryptoResult<()> {
        let mut l1 = TokenLedger::new();
        let mut l2 = TokenLedger::new();
        let t1 = l1.mint(1, &cids(3), T0, T1, T0, serde_json::Value::Null)?;
        let t2 = l2.mint(1, &cids(3), T0, T1, T0, serde_json::Value::Null)?;
        // Same inputs → bit-identical token (id, root, score, mint time).
        assert_eq!(t1.token_id, t2.token_id);
        assert_eq!(t1.chained_root, t2.chained_root);
        assert_eq!(t1.merit_score, t2.merit_score);
        assert!(t1.token_id.starts_with("GT-"));
        assert_eq!(t1.event_count, 3);
        assert_eq!(t1.status, TokenStatus::Minted);
        Ok(())
    }

    #[test]
    fn double_mint_same_batch_is_rejected() -> CryptoResult<()> {
        let mut l = TokenLedger::new();
        l.mint(1, &cids(2), T0, T1, T0, serde_json::Value::Null)?;
        assert!(l
            .mint(1, &cids(2), T0, T1, T0, serde_json::Value::Null)
            .is_err());
        Ok(())
    }

    #[test]
    fn lifecycle_transitions_are_enforced() -> CryptoResult<()> {
        let mut l = TokenLedger::new();
        let t = l.mint(7, &cids(1), T0, T1, T0, serde_json::Value::Null)?;
        let id = t.token_id.clone();

        // Spending a merely-minted token is now a typed error (legacy SQL no-op'd).
        assert!(l.spend(&id).is_err());

        l.activate(&id)?;
        assert_eq!(l.get(&id)?.status, TokenStatus::Active);
        l.spend(&id)?;
        assert_eq!(l.get(&id)?.status, TokenStatus::Spent);

        // Cannot re-activate a spent token.
        assert!(l.activate(&id).is_err());
        Ok(())
    }

    #[test]
    fn burn_and_revoke() -> CryptoResult<()> {
        let mut l = TokenLedger::new();
        let id = l
            .mint(1, &cids(1), T0, T1, T0, serde_json::Value::Null)?
            .token_id;
        l.burn(&id, "test burn")?;
        assert_eq!(l.get(&id)?.status, TokenStatus::Burned);
        assert_eq!(l.count_by_status(TokenStatus::Burned), 1);

        let id2 = l
            .mint(2, &cids(1), T0, T1, T0, serde_json::Value::Null)?
            .token_id;
        l.revoke(&id2, "admin")?;
        assert_eq!(l.get(&id2)?.status, TokenStatus::Revoked);
        // Revoked is terminal — no further transitions.
        assert!(l.activate(&id2).is_err());
        Ok(())
    }

    #[test]
    fn listing_and_ranking_by_merit() -> CryptoResult<()> {
        let mut l = TokenLedger::new();
        l.mint(1, &cids(10), T0, T1, T0, serde_json::Value::Null)?;
        l.mint(2, &cids(5), T0, T1, T0, serde_json::Value::Null)?;
        l.mint(3, &cids(1), T0, T1, T0, serde_json::Value::Null)?;
        assert_eq!(l.total(), 3);
        assert_eq!(l.list_active(10).len(), 3);

        let top = l.top_by_merit(3);
        assert_eq!(top.len(), 3);
        assert_eq!(top[0].rank, 1);
        assert!(top[0].merit_score > top[1].merit_score); // 10-event token leads
        Ok(())
    }

    #[test]
    fn missing_token_is_typed_error() {
        let l = TokenLedger::new();
        assert!(l.get("GT-nope").is_err());
    }
}
