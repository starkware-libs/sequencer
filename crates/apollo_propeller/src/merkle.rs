//! Merkle tree implementation for efficient data integrity verification.
//!
//! This module provides a simple and efficient Merkle tree implementation that can be used
//! to create cryptographic proofs for shards in the propeller protocol.

use sha2::{Digest, Sha256};

/// A hash value in the Merkle tree (32 bytes from SHA-256).
pub type MerkleHash = [u8; 32];

/// Default root of an empty tree
pub const EMPTY_TREE_ROOT: MerkleHash = [0u8; 32];

/// A Merkle tree for verifying data integrity.
#[derive(Debug, Clone)]
pub struct MerkleTree {
    /// All nodes in the tree, stored level by level from bottom to top.
    /// Index 0 contains the leaves, and the last element contains the root.
    nodes: Vec<Vec<MerkleHash>>,
}

/// A Merkle proof that can be used to verify a leaf is part of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MerkleProof {
    /// The sibling hashes needed to reconstruct the path to the root.
    pub siblings: Vec<MerkleHash>,
}

impl MerkleTree {
    /// Hash a data chunk to create a leaf hash.
    pub fn hash_leaf(data: &[u8]) -> MerkleHash {
        let mut hasher = Sha256::new();
        hasher.update(b"<leaf>");
        hasher.update(data);
        hasher.update(b"</leaf>");
        hasher.finalize().into()
    }

    /// Get the root hash of the tree.
    pub fn root(&self) -> MerkleHash {
        self.nodes.last().map(|level| level[0]).unwrap_or(EMPTY_TREE_ROOT)
    }

    /// Get the number of leaves in the tree.
    pub fn leaf_count(&self) -> usize {
        self.nodes.first().map(|level| level.len()).unwrap_or(0)
    }

    /// Get the leaves of the tree.
    pub fn leaves(&self) -> &[MerkleHash] {
        self.nodes.first().map(|level| level.as_slice()).unwrap_or(&[])
    }

    /// Generate a Merkle proof for a specific leaf index.
    ///
    /// Returns `None` if the index is out of bounds.
    pub fn prove(&self, leaf_index: usize) -> Option<MerkleProof> {
        let leaves = self.nodes.first()?;
        if leaf_index >= leaves.len() {
            return None;
        }

        let mut siblings = Vec::new();
        let mut index = leaf_index;

        // Iterate through levels from bottom to top (excluding the root)
        for level in &self.nodes[..self.nodes.len() - 1] {
            let level_size = level.len();

            // Check if sibling exists
            if level_size > 1 {
                // XOR the index with 1 to get the sibling index
                let sibling_index = index ^ 1;

                // Add sibling hash
                // If sibling doesn't exist (odd node at end), use current node (it was duplicated)
                let sibling_hash =
                    if sibling_index < level_size { level[sibling_index] } else { level[index] };
                siblings.push(sibling_hash);
            }

            // Move to parent level
            index = index >> 1;
        }

        Some(MerkleProof { siblings })
    }

    // fn prove_all()

    /// Verify a Merkle proof against this tree's root.
    pub fn verify(&self, leaf_hash: &MerkleHash, proof: &MerkleProof, leaf_index: usize) -> bool {
        proof.verify(&self.root(), leaf_hash, leaf_index)
    }
}

/// Hash a pair of nodes to create a parent hash.
fn hash_pair(left: &MerkleHash, right: &MerkleHash) -> MerkleHash {
    let mut hasher = Sha256::new();
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
            index = index >> 1;
        }

        current_hash == *root
    }
}
