//! Merkle Mountain Range — append-only tree for O(log n) inclusion proofs.
//!
//! The accumulator behind the unified ledger's mathematical provability: each
//! record's content hash is appended as a leaf, and the MMR root is the Merkle
//! root of all peaks, giving tamper-evident O(log n) inclusion proofs.
//!
//! Ported from the bottom-up scaffold and brought to the manifesto bar: the
//! `merkle_store` submodule (storage integration — an engine-layer concern) is
//! removed, and the two runtime `.unwrap()`s in `proof()` are eradicated. Runtime
//! paths are now `unwrap`-free; only `#[cfg(test)]` code uses `unwrap`.

use blake3::Hasher;
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

/// Merkle Mountain Range errors.
#[derive(Debug, Error)]
pub enum MmrError {
    #[error("index out of bounds: {0}")]
    IndexOutOfBounds(usize),
    #[error("empty MMR: no leaves")]
    Empty,
    #[error("proof verification failed")]
    ProofVerificationFailed,
    #[error("root mismatch: expected {expected}, got {actual}")]
    RootMismatch { expected: String, actual: String },
}

/// A single peak in the MMR (a Merkle subtree root).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Peak(pub [u8; 32]);

/// A Merkle Mountain Range.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleForest {
    leaves: Vec<[u8; 32]>,
    peaks: Vec<Peak>,
    count: u64,
}

impl MerkleForest {
    pub fn new() -> Self {
        Self {
            leaves: vec![],
            peaks: vec![],
            count: 0,
        }
    }

    pub fn append(&mut self, leaf: [u8; 32]) {
        self.leaves.push(leaf);
        self.count += 1;
        self.recompute_peaks();
    }

    pub fn append_data(&mut self, data: &[u8]) -> [u8; 32] {
        let hash = blake3::hash(data).as_bytes().to_owned();
        self.append(hash);
        hash
    }

    pub fn root(&self) -> [u8; 32] {
        if self.peaks.is_empty() {
            return [0u8; 32];
        }
        let mut hasher = Hasher::new();
        for peak in &self.peaks {
            hasher.update(&peak.0);
        }
        hasher.finalize().into()
    }

    pub fn len(&self) -> usize {
        self.leaves.len()
    }
    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }
    pub fn peaks(&self) -> &[Peak] {
        &self.peaks
    }
    pub fn leaf_count(&self) -> usize {
        self.leaves.len()
    }

    pub fn get_leaf(&self, index: usize) -> Result<[u8; 32], MmrError> {
        if index >= self.leaves.len() {
            return Err(MmrError::IndexOutOfBounds(index));
        }
        Ok(self.leaves[index])
    }

    fn find_peak_for_leaf(&self, index: usize) -> Result<(usize, usize, usize), MmrError> {
        let n = self.leaves.len();
        if index >= n {
            return Err(MmrError::IndexOutOfBounds(index));
        }
        let mut peak_idx = 0;
        let mut offset = 0;
        while offset < n {
            let remaining = n - offset;
            let mut size = 1;
            while size * 2 <= remaining {
                size *= 2;
            }
            if index < offset + size {
                return Ok((peak_idx, index - offset, size));
            }
            offset += size;
            peak_idx += 1;
        }
        Err(MmrError::IndexOutOfBounds(index))
    }

    pub fn proof(&self, index: usize) -> Result<Vec<[u8; 32]>, MmrError> {
        let (_peak_idx, local_idx, mountain_size) = self.find_peak_for_leaf(index)?;
        let start = index - local_idx;

        // Build all intermediate levels (unwrap-free: track `level` directly).
        let mut level: Vec<[u8; 32]> = (start..start + mountain_size).map(|i| self.leaves[i]).collect();
        let mut levels: Vec<Vec<[u8; 32]>> = vec![level.clone()];
        while level.len() > 1 {
            let mut next = Vec::with_capacity(level.len().div_ceil(2));
            for chunk in level.chunks(2) {
                if chunk.len() == 2 {
                    next.push(Self::hash_pair(chunk[0], chunk[1]));
                } else {
                    next.push(chunk[0]);
                }
            }
            level = next;
            levels.push(level.clone());
        }

        // Extract siblings along the path to the peak.
        let mut proof = vec![];
        let mut pos = local_idx;
        for nodes in &levels[..levels.len() - 1] {
            if pos % 2 == 0 && pos + 1 < nodes.len() {
                proof.push(nodes[pos + 1]);
            } else if pos % 2 == 1 {
                proof.push(nodes[pos - 1]);
            }
            pos /= 2;
        }
        Ok(proof)
    }

    pub fn verify(&self, leaf: [u8; 32], proof: &[[u8; 32]], index: usize) -> bool {
        let (peak_idx, local_idx, _) = match self.find_peak_for_leaf(index) {
            Ok(v) => v,
            Err(_) => return false,
        };

        let mut current = leaf;
        let mut pos = local_idx;
        for sibling in proof {
            if pos % 2 == 0 {
                current = Self::hash_pair(current, *sibling);
            } else {
                current = Self::hash_pair(*sibling, current);
            }
            pos /= 2;
        }

        let mut all_peaks: Vec<[u8; 32]> = self.peaks.iter().map(|p| p.0).collect();
        if peak_idx < all_peaks.len() {
            all_peaks[peak_idx] = current;
        }
        let mut hasher = Hasher::new();
        for peak in &all_peaks {
            hasher.update(peak);
        }
        *hasher.finalize().as_bytes() == self.root()
    }

    pub fn verify_with_root(
        &self,
        leaf: [u8; 32],
        proof: &[[u8; 32]],
        index: usize,
        expected_root: [u8; 32],
    ) -> Result<(), MmrError> {
        let (peak_idx, local_idx, _) = self.find_peak_for_leaf(index)?;

        let mut current = leaf;
        let mut pos = local_idx;
        for sibling in proof {
            if pos % 2 == 0 {
                current = Self::hash_pair(current, *sibling);
            } else {
                current = Self::hash_pair(*sibling, current);
            }
            pos /= 2;
        }

        let mut all_peaks: Vec<[u8; 32]> = self.peaks.iter().map(|p| p.0).collect();
        if peak_idx < all_peaks.len() {
            all_peaks[peak_idx] = current;
        }
        let mut hasher = Hasher::new();
        for peak in &all_peaks {
            hasher.update(peak);
        }
        let computed_root: [u8; 32] = hasher.finalize().into();

        if computed_root != expected_root {
            return Err(MmrError::RootMismatch {
                expected: hex::encode(expected_root),
                actual: hex::encode(computed_root),
            });
        }
        Ok(())
    }

    pub fn compact_proof(&self, index: usize) -> Result<CompactProof, MmrError> {
        Ok(CompactProof {
            leaf: self.get_leaf(index)?,
            proof: self.proof(index)?,
            root: self.root(),
            index,
        })
    }

    pub fn verify_compact(&self, compact: &CompactProof) -> bool {
        self.verify(compact.leaf, &compact.proof, compact.index)
    }

    fn recompute_peaks(&mut self) {
        self.peaks.clear();
        let n = self.leaves.len();
        let mut i = 0;
        while i < n {
            let remaining = n - i;
            let mut size = 1;
            while size * 2 <= remaining {
                size *= 2;
            }
            let peak = self.build_merkle_tree(i, size);
            self.peaks.push(Peak(peak));
            i += size;
        }
    }

    fn build_merkle_tree(&self, start: usize, size: usize) -> [u8; 32] {
        if size == 1 {
            return self.leaves[start];
        }
        let mut level: Vec<[u8; 32]> = (start..start + size).map(|i| self.leaves[i]).collect();
        while level.len() > 1 {
            let mut next = Vec::new();
            for chunk in level.chunks(2) {
                if chunk.len() == 2 {
                    next.push(Self::hash_pair(chunk[0], chunk[1]));
                } else {
                    next.push(chunk[0]);
                }
            }
            level = next;
        }
        level[0]
    }

    pub fn hash_pair(a: [u8; 32], b: [u8; 32]) -> [u8; 32] {
        let mut hasher = Hasher::new();
        hasher.update(&a);
        hasher.update(&b);
        hasher.finalize().into()
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "leaves": self.leaves.iter().map(hex::encode).collect::<Vec<_>>(),
            "peaks": self.peaks.iter().map(|p| hex::encode(p.0)).collect::<Vec<_>>(),
            "root": hex::encode(self.root()),
            "count": self.count,
        })
    }

    pub fn from_json(json: &serde_json::Value) -> Result<Self, MmrError> {
        let leaves = decode_hash_array(json["leaves"].as_array().ok_or(MmrError::Empty)?)?;
        let peaks = decode_hash_array(json["peaks"].as_array().ok_or(MmrError::Empty)?)?
            .into_iter()
            .map(Peak)
            .collect();
        Ok(Self {
            leaves,
            peaks,
            count: json["count"].as_u64().unwrap_or(0),
        })
    }
}

fn decode_hash_array(items: &[serde_json::Value]) -> Result<Vec<[u8; 32]>, MmrError> {
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let s = item.as_str().ok_or(MmrError::Empty)?;
        let bytes = hex::decode(s).map_err(|_| MmrError::ProofVerificationFailed)?;
        let arr: [u8; 32] = bytes.try_into().map_err(|_| MmrError::ProofVerificationFailed)?;
        out.push(arr);
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactProof {
    pub leaf: [u8; 32],
    pub proof: Vec<[u8; 32]>,
    pub root: [u8; 32],
    pub index: usize,
}

impl fmt::Display for CompactProof {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CompactProof(index={}, root={}, proof_len={})",
            self.index,
            hex::encode(self.root),
            self.proof.len()
        )
    }
}

impl Default for MerkleForest {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_mmr() {
        let mmr = MerkleForest::new();
        assert!(mmr.is_empty());
        assert_eq!(mmr.root(), [0u8; 32]);
    }

    #[test]
    fn proof_verification() {
        let mut mmr = MerkleForest::new();
        let h0 = mmr.append_data(b"leaf0");
        let h1 = mmr.append_data(b"leaf1");
        let _h2 = mmr.append_data(b"leaf2");
        let h3 = mmr.append_data(b"leaf3");
        assert!(mmr.verify(h0, &mmr.proof(0).unwrap(), 0));
        assert!(mmr.verify(h3, &mmr.proof(3).unwrap(), 3));
        assert!(mmr.verify(h1, &mmr.proof(1).unwrap(), 1));
    }

    #[test]
    fn root_is_deterministic() {
        let mut m1 = MerkleForest::new();
        let mut m2 = MerkleForest::new();
        for b in &[b"a", b"b", b"c"] {
            m1.append_data(*b);
            m2.append_data(*b);
        }
        assert_eq!(m1.root(), m2.root());
    }

    #[test]
    fn json_roundtrip() {
        let mut mmr = MerkleForest::new();
        for b in &[b"leaf0", b"leaf1", b"leaf2"] {
            mmr.append_data(*b);
        }
        let mmr2 = MerkleForest::from_json(&mmr.to_json()).unwrap();
        assert_eq!(mmr.root(), mmr2.root());
        assert_eq!(mmr.len(), mmr2.len());
    }

    #[test]
    fn many_leaves_all_verify() {
        let mut mmr = MerkleForest::new();
        for i in 0..100 {
            mmr.append_data(format!("tablet_{i}").as_bytes());
        }
        assert_eq!(mmr.len(), 100);
        for i in 0..100 {
            let leaf = mmr.get_leaf(i).unwrap();
            assert!(mmr.verify(leaf, &mmr.proof(i).unwrap(), i));
        }
    }

    #[test]
    fn verify_wrong_leaf_fails() {
        let mut mmr = MerkleForest::new();
        for b in &[b"leaf0", b"leaf1", b"leaf2"] {
            mmr.append_data(*b);
        }
        let proof = mmr.proof(0).unwrap();
        assert!(!mmr.verify([0u8; 32], &proof, 0));
    }

    #[test]
    fn verify_wrong_root_errors() {
        let mut mmr = MerkleForest::new();
        let h0 = mmr.append_data(b"leaf0");
        mmr.append_data(b"leaf1");
        let proof = mmr.proof(0).unwrap();
        assert!(mmr.verify_with_root(h0, &proof, 0, [1u8; 32]).is_err());
    }

    #[test]
    fn out_of_bounds_is_error() {
        let mut mmr = MerkleForest::new();
        mmr.append_data(b"leaf0");
        assert!(mmr.proof(5).is_err());
        assert!(mmr.get_leaf(9).is_err());
    }
}
