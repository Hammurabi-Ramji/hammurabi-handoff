//! RAMesh — the Readable Agent Mesh substrate.
//!
//! An IRC-style, append-only message feed. Every actor in the pipeline
//! (agents, the x402 gate, the Handshake vault, the settlement relay)
//! posts human-readable lines to a channel the operator can monitor.
//! This is the "Legible Autonomy" contract: if it isn't in the feed,
//! it didn't happen.
//!
//! The bus is deliberately simple — an in-memory log plus a broadcast
//! stream for live subscribers (the Tauri frontend). No persistence, no
//! egress. Chat timestamps use the ambient clock (they are observability,
//! not cryptography — signing paths take caller-supplied time).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// The operations channel every pipeline actor posts to.
pub const OPS_CHANNEL: &str = "#handoff-ops";

/// Capacity of the live broadcast ring — slow subscribers lag, they never
/// block a post (the log remains the source of truth).
const LIVE_CAPACITY: usize = 256;

/// Cap on the retained log. RAMesh is a live audit feed for one operator
/// session, not a database — without a bound, a long-running process (or a
/// script hammering `/agent/announce` in a loop) grows the log forever.
/// `seq` numbering stays monotonic and gap-aware even as old lines are
/// evicted, so a client that fetches `feed()` after missing entries can
/// tell it has a gap (its lowest `seq` is nonzero) rather than mistake a
/// trimmed feed for a fresh one.
const MAX_LOG_LEN: usize = 2_000;

/// Classification of a mesh line, so the frontend can style/filter without
/// parsing prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeshEventKind {
    /// An agent announcing an opportunity / requesting execution.
    Intent,
    /// The x402 gate refusing an unpaid request.
    Restriction,
    /// A payment clearing.
    Payment,
    /// The Handshake vault minting a credential.
    Credential,
    /// A settlement envelope leaving for the relay (stubbed).
    Settlement,
    /// Anything else (startup notices, diagnostics).
    Info,
}

/// One line in the mesh feed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshMessage {
    /// Monotonic sequence number (total order of the feed).
    pub seq: u64,
    /// IRC-style channel, e.g. `#handoff-ops`.
    pub channel: String,
    /// IRC-style handle of the poster, e.g. `agent/scout-1`, `gate/x402`.
    pub sender: String,
    /// Machine-readable classification of the line.
    pub kind: MeshEventKind,
    /// The human-readable body — the thing the operator actually reads.
    pub body: String,
    /// RFC 3339 wall-clock timestamp (display only).
    pub at: String,
}

impl MeshMessage {
    /// Render as a classic IRC line: `[seq] <sender> body`.
    pub fn render(&self) -> String {
        format!("[{:>4}] <{}> {}", self.seq, self.sender, self.body)
    }
}

/// The mesh bus: append-only log + live broadcast.
pub struct MeshBus {
    seq: AtomicU64,
    log: RwLock<Vec<MeshMessage>>,
    live: broadcast::Sender<MeshMessage>,
}

impl MeshBus {
    /// Create an empty bus.
    pub fn new() -> Self {
        let (live, _) = broadcast::channel(LIVE_CAPACITY);
        Self {
            seq: AtomicU64::new(0),
            log: RwLock::new(Vec::new()),
            live,
        }
    }

    /// Post a line to [`OPS_CHANNEL`]. Returns the recorded message.
    ///
    /// Never fails: if no live subscriber is listening the broadcast send is
    /// a no-op; the log always records the line.
    pub fn post(&self, sender: &str, kind: MeshEventKind, body: &str) -> MeshMessage {
        let msg = MeshMessage {
            seq: self.seq.fetch_add(1, Ordering::SeqCst),
            channel: OPS_CHANNEL.to_string(),
            sender: sender.to_string(),
            kind,
            body: body.to_string(),
            at: Utc::now().to_rfc3339(),
        };
        {
            let mut log = self.log.write().unwrap_or_else(|e| e.into_inner());
            log.push(msg.clone());
            if log.len() > MAX_LOG_LEN {
                let excess = log.len() - MAX_LOG_LEN;
                log.drain(0..excess);
            }
        }
        let _ = self.live.send(msg.clone());
        msg
    }

    /// Snapshot of the whole feed, in posting order.
    pub fn feed(&self) -> Vec<MeshMessage> {
        self.log.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Subscribe to live messages (posts made after this call).
    pub fn subscribe(&self) -> broadcast::Receiver<MeshMessage> {
        self.live.subscribe()
    }

    /// The feed rendered as an IRC transcript — one line per message.
    pub fn transcript(&self) -> String {
        self.feed()
            .iter()
            .map(MeshMessage::render)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl Default for MeshBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posts_are_totally_ordered_and_logged() {
        let bus = MeshBus::new();
        bus.post("agent/scout-1", MeshEventKind::Intent, "first");
        bus.post("gate/x402", MeshEventKind::Restriction, "second");
        let feed = bus.feed();
        assert_eq!(feed.len(), 2);
        assert_eq!(feed[0].seq, 0);
        assert_eq!(feed[1].seq, 1);
        assert_eq!(feed[0].body, "first");
        assert_eq!(feed[1].channel, OPS_CHANNEL);
    }

    #[tokio::test]
    async fn live_subscribers_see_new_posts() {
        let bus = MeshBus::new();
        let mut rx = bus.subscribe();
        bus.post("vault/handshake", MeshEventKind::Credential, "minted");
        let live = rx.recv().await.expect("live message");
        assert_eq!(live.body, "minted");
        assert_eq!(live.kind, MeshEventKind::Credential);
    }

    #[test]
    fn transcript_renders_irc_style() {
        let bus = MeshBus::new();
        bus.post("agent/scout-1", MeshEventKind::Intent, "hello");
        assert!(bus.transcript().contains("<agent/scout-1> hello"));
    }

    #[test]
    fn log_is_capped_and_seq_stays_monotonic_across_the_trim() {
        let bus = MeshBus::new();
        let overflow = 50;
        for i in 0..(MAX_LOG_LEN + overflow) {
            bus.post("agent/scout-1", MeshEventKind::Info, &format!("line {i}"));
        }
        let feed = bus.feed();
        assert_eq!(feed.len(), MAX_LOG_LEN, "log must not grow past the cap");
        // The oldest `overflow` lines were evicted — the retained window
        // starts right where they left off, seq numbering unbroken.
        assert_eq!(feed[0].seq, overflow as u64);
        assert_eq!(feed[0].body, format!("line {overflow}"));
        let last = feed.last().unwrap();
        assert_eq!(last.seq, (MAX_LOG_LEN + overflow - 1) as u64);
    }
}
