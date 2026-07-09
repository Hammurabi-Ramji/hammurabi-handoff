//! `mmr` — Merkle Mountain Range entrypoint.
//!
//! The MMR accumulator implementation lives in [`crate::merkle`]; this module
//! re-exports it so the absorbed `iedb-mmr` surface
//! (`iedb_crypto::mmr::MerkleForest`) stays stable for downstream callers.

pub use crate::merkle::{CompactProof, MerkleForest, MmrError, Peak};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
