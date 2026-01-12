//! Merkle tree implementation for efficient data integrity verification.
//!
//! This module provides a simple and efficient Merkle tree implementation that can be used
//! to create cryptographic proofs for shards in the propeller protocol.

use sha2::{Digest, Sha256};

/// A hash value in the Merkle tree (32 bytes from SHA-256).
pub type MerkleHash = [u8; 32];

/// A Merkle proof that can be used to verify a leaf is part of the tree.
///
/// MerkleProof is succinct because it is sent over the wire, so it doesn't contain proof
/// metadata like the root, leaf_hash and leaf_index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MerkleProof {
    /// The sibling hashes needed to reconstruct the path to the root.
    pub siblings: Vec<MerkleHash>,
}

/// Hash a pair of nodes to create a parent hash.
fn hash_pair(left: &MerkleHash, right: &MerkleHash) -> MerkleHash {
    let mut hasher = Sha256::new();
    // TODO(AndrewL): Talk with the team about these hash wrappings and what to do with them.
    hasher.update(b"<node>");
    hasher.update(b"<left>");
    hasher.update(left);
    hasher.update(b"</left>");
    hasher.update(b"<right>");
    hasher.update(right);
    hasher.update(b"</right>");
    hasher.update(b"</node>");
    hasher.finalize().into()
}

impl MerkleProof {
    /// Verify a Merkle proof against a known root hash.
    pub fn verify(&self, root: &MerkleHash, leaf_hash: &MerkleHash, leaf_index: usize) -> bool {
        let mut current_hash = *leaf_hash;
        let mut index = leaf_index;

        for sibling in &self.siblings {
            current_hash = if index.is_multiple_of(2) {
                hash_pair(&current_hash, sibling)
            } else {
                hash_pair(sibling, &current_hash)
            };
            index /= 2;
        }

        current_hash == *root
    }
}
