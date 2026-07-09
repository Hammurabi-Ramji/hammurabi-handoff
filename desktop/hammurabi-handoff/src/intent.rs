//! Agent intents — the structured payloads behind the readable chat lines.
//!
//! An agent never acts silently: it posts a human-readable announcement to
//! RAMesh *and* carries a structured [`AgentIntent`] payload that the rest
//! of the pipeline (x402 gate → Handshake vault → settlement formatter)
//! operates on. The payload is what ultimately gets canonicalized
//! (RFC 8785) and signature-bound, so it must be canonicalizable:
//! **no floats** — money is integer atomic units, quantities are decimal
//! strings.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ulid::Ulid;

/// The exact Phase-1 announcement line (Legible Autonomy narrative string —
/// do not reword).
pub const INTENT_ANNOUNCEMENT: &str =
    "Opportunity found: ETH limit order at $3,100. Requesting execution.";

/// The mock agent's IRC handle.
pub const MOCK_AGENT_HANDLE: &str = "agent/scout-1";

/// Order side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderSide {
    Buy,
    Sell,
}

/// A limit order. All numeric fields are canonicalization-safe:
/// integer cents / wei strings, never floats (RFC 8785 profile in
/// `iedb-crypto::jcs` rejects floats outright).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LimitOrder {
    /// Trading pair, e.g. `ETH/USDC`.
    pub pair: String,
    pub side: OrderSide,
    /// Limit price in integer USD cents (310_000 = $3,100.00).
    pub limit_price_usd_cents: u64,
    /// Quantity in wei, as a decimal string (avoids u128/float pitfalls in
    /// JSON round-trips).
    pub quantity_wei: String,
    /// Venue label — always a local simulation in this build.
    pub venue: String,
}

/// A structured agent intent: the payload the Handshake vault will
/// signature-bind after the x402 gate clears.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentIntent {
    /// ULID — sortable, locally minted.
    pub intent_id: String,
    /// IRC handle of the announcing agent.
    pub agent_handle: String,
    /// The privileged action being requested.
    pub action: String,
    pub order: LimitOrder,
}

impl AgentIntent {
    /// The Phase-1 mock: one ETH limit order at $3,100.
    pub fn mock_eth_limit_order() -> Self {
        Self {
            intent_id: Ulid::new().to_string(),
            agent_handle: MOCK_AGENT_HANDLE.to_string(),
            action: "execute_limit_order".to_string(),
            order: LimitOrder {
                pair: "ETH/USDC".to_string(),
                side: OrderSide::Buy,
                limit_price_usd_cents: 310_000,
                quantity_wei: "1000000000000000000".to_string(), // 1 ETH
                venue: "local-simulated".to_string(),
            },
        }
    }

    /// The canonical signing payload — exactly what the vault binds the
    /// ephemeral signature to. Serialization of a float-free struct cannot
    /// fail, hence the infallible signature.
    pub fn payload(&self) -> Value {
        serde_json::to_value(self).expect("float-free intent always serializes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_intent_matches_the_narrative() {
        let intent = AgentIntent::mock_eth_limit_order();
        assert_eq!(intent.order.limit_price_usd_cents, 310_000); // $3,100
        assert_eq!(intent.order.pair, "ETH/USDC");
        assert_eq!(intent.action, "execute_limit_order");
        assert!(!intent.intent_id.is_empty());
    }

    #[test]
    fn payload_is_canonicalizable() {
        let intent = AgentIntent::mock_eth_limit_order();
        // The whole point of integer-cents/wei-strings: the RFC 8785 profile
        // must accept the payload.
        iedb_crypto::jcs::canonicalize(&intent.payload())
            .expect("intent payload must be canonicalization-safe");
    }

    #[test]
    fn payload_round_trips() {
        let intent = AgentIntent::mock_eth_limit_order();
        let back: AgentIntent = serde_json::from_value(intent.payload()).expect("round trip");
        assert_eq!(back, intent);
    }
}
