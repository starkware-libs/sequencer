//! Merkle tree implementation for efficient data integrity verification.
//!
//! This module provides a simple and efficient Merkle tree implementation that can be used
//! to create cryptographic proofs for shards in the propeller protocol.

use sha2::{Digest, Sha256};

/// A hash value in the Merkle tree (32 bytes from SHA-256).
pub type MerkleHash = [u8; 32];

/// A Merkle tree for verifying data integrity.
#[derive(Debug, Clone)]
pub struct MerkleTree {
    /// All nodes in the tree, stored level by level from bottom to top.
    /// Index 0 contains the leaves, and the last element contains the root.
    nodes_by_level: Vec<Vec<MerkleHash>>,
}

/// A Merkle proof that can be used to verify a leaf is part of the tree.
///
/// MerkleProof is succinct because it is sent over the wire, so it doesn't contain proof
/// metadata like the root, leaf_hash and leaf_index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MerkleProof {
    /// The sibling hashes needed to reconstruct the path to the root.
    pub siblings: Vec<MerkleHash>,
}

impl MerkleTree {
    /// Hash a data chunk to create a leaf hash.
    pub fn hash_leaf(data: &[u8]) -> MerkleHash {
        let mut hasher = Sha256::new();
        // TODO(AndrewL): Talk with the team about these hash wrappings and what to do with them.
        hasher.update(b"<leaf>");
        hasher.update(data);
        hasher.update(b"</leaf>");
        hasher.finalize().into()
    }

    /// Create a new Merkle tree from data chunks.
    ///
    /// Each chunk is hashed to create a leaf, and the tree is built bottom-up.
    pub fn new(data_chunks: &[Vec<u8>]) -> Self {
        let mut leaves: Vec<MerkleHash> = Vec::with_capacity(data_chunks.len());
        for chunk in data_chunks {
            leaves.push(Self::hash_leaf(chunk));
        }
        Self::from_leaves(leaves)
    }

    /// Create a new Merkle tree from pre-computed leaf hashes.
    fn from_leaves(leaves: Vec<MerkleHash>) -> Self {
        if leaves.is_empty() {
            return Self { nodes_by_level: vec![] };
        }

        // Build tree level by level
        let mut levels = vec![];
        let mut current_level = leaves;

        loop {
            levels.push(current_level.clone());

            if current_level.len() <= 1 {
                break;
            }

            let mut next_level = Vec::new();

            for chunk in current_level.chunks(2) {
                let left = chunk.first().expect("Vec's chunks never returns empty chunks");
                let right = chunk.last().expect("Vec's chunks never returns empty chunks");

                if chunk.len() == 1 {
                    next_level.push(*left);
                } else {
                    next_level.push(hash_pair(left, right));
                }
            }

            current_level = next_level;
        }

        Self { nodes_by_level: levels }
    }

    /// Get the root hash of the tree.
    /// Returns `None` if the tree is empty.
    pub fn root(&self) -> Option<MerkleHash> {
        self.nodes_by_level.last().and_then(|level| level.first().copied())
    }

    /// Get the number of leaves in the tree.
    pub fn leaf_count(&self) -> usize {
        self.nodes_by_level.first().map(|level| level.len()).unwrap_or(0)
    }

    /// Get the leaves of the tree.
    /// Returns `None` if the tree is empty.
    pub fn leaves(&self) -> Option<&[MerkleHash]> {
        self.nodes_by_level.first().map(|level| level.as_slice())
    }

    /// Generate a Merkle proof for a specific leaf index.
    ///
    /// Returns `None` if the index is out of bounds.
    pub fn prove(&self, leaf_index: usize) -> Option<MerkleProof> {
        let leaves = self.nodes_by_level.first()?;
        if leaf_index >= leaves.len() {
            return None;
        }

        let mut siblings = Vec::new();
        let mut index = leaf_index;

        // Iterate through levels from bottom to top (excluding the root)
        for level in &self.nodes_by_level[..self.nodes_by_level.len() - 1] {
            let level_size = level.len();

            // Check if sibling exists
            if level_size <= 1 {
                index /= 2;
                continue;
            }

            // If the index is even, take the left sibling, else take the right sibling
            let sibling_index = index ^ 1;

            // Add sibling hash
            debug_assert!(index < level_size);
            debug_assert!(sibling_index <= level_size);
            // this sibling is over the edge of the tree so we don't take anything on this level
            if sibling_index != level_size {
                siblings.push(level[sibling_index]);
            }

            // Move to parent level
            index /= 2;
        }

        Some(MerkleProof { siblings })
    }

    /// Verify a Merkle proof against this tree's root.
    pub fn verify(
        &self,
        leaf_hash: &MerkleHash,
        proof: &MerkleProof,
        leaf_index: usize,
    ) -> Option<bool> {
        self.root().map(|root| proof.verify(&root, leaf_hash, leaf_index, self.leaf_count()))
    }
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
    pub fn verify(
        &self,
        root: &MerkleHash,
        leaf_hash: &MerkleHash,
        leaf_index: usize,
        leaf_count: usize,
    ) -> bool {
        let mut current_hash = *leaf_hash;
        let mut index = leaf_index;
        let mut level_size = leaf_count;

        for sibling in &self.siblings {
            // Skip levels where this node has no sibling (promoted directly)
            while level_size > 1 {
                let sibling_index = index ^ 1;
                if sibling_index >= level_size {
                    // No sibling at this level, node is promoted
                    index /= 2;
                    level_size = level_size.div_ceil(2);
                } else {
                    // Has a sibling at this level
                    break;
                }
            }

            current_hash = if index.is_multiple_of(2) {
                hash_pair(&current_hash, sibling)
            } else {
                hash_pair(sibling, &current_hash)
            };

            index /= 2;
            level_size = level_size.div_ceil(2);
        }

        current_hash == *root
    }
}
