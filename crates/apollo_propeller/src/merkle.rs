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
    /// The leaves of the tree (hashes of the data chunks).
    leaves: Vec<MerkleHash>,
    /// All nodes in the tree, stored level by level.
    /// Index 0 is the root, and subsequent levels are stored in order.
    nodes: Vec<MerkleHash>,
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

    /// Create a new Merkle tree from data chunks.
    ///
    /// Each chunk is hashed to create a leaf, and the tree is built bottom-up.
    pub fn new(data_chunks: &[Vec<u8>]) -> Self {
        let leaves: Vec<MerkleHash> =
            data_chunks.iter().map(|chunk| Self::hash_leaf(chunk)).collect();
        Self::from_leaves(leaves)
    }

    /// Create a new Merkle tree from pre-computed leaf hashes.
    pub fn from_leaves(leaves: Vec<MerkleHash>) -> Self {
        if leaves.is_empty() {
            return Self { leaves: vec![], nodes: vec![[0u8; 32]] };
        }

        let nodes = build_tree(&leaves);
        Self { leaves, nodes }
    }

    /// Get the root hash of the tree.
    pub fn root(&self) -> MerkleHash {
        self.nodes[0]
    }

    /// Get the number of leaves in the tree.
    pub fn leaf_count(&self) -> usize {
        self.leaves.len()
    }

    /// Generate a Merkle proof for a specific leaf index.
    ///
    /// Returns `None` if the index is out of bounds.
    pub fn prove(&self, leaf_index: usize) -> Option<MerkleProof> {
        if leaf_index >= self.leaves.len() {
            return None;
        }

        let mut siblings = Vec::new();
        let mut index = leaf_index;
        let mut level_size = self.leaves.len();

        // Track position in the nodes array
        let mut level_start = self.nodes.len() - self.leaves.len();

        while level_size > 1 {
            // Find sibling index
            let sibling_index = if index % 2 == 0 { index + 1 } else { index - 1 };

            // Add sibling hash
            // If sibling doesn't exist (odd node at end), use current node (it was duplicated)
            let sibling_hash = if sibling_index < level_size {
                self.nodes[level_start + sibling_index]
            } else {
                self.nodes[level_start + index]
            };
            siblings.push(sibling_hash);

            // Move to parent level
            index /= 2;
            level_start -= level_size.div_ceil(2);
            level_size = level_size.div_ceil(2);
        }

        Some(MerkleProof { siblings })
    }

    // fn prove_all()

    /// Verify a Merkle proof against this tree's root.
    pub fn verify(&self, leaf_hash: &MerkleHash, proof: &MerkleProof, leaf_index: usize) -> bool {
        proof.verify(&self.root(), leaf_hash, leaf_index)
    }
}

impl MerkleProof {
    pub fn serialize(&self) -> Vec<u8> {
        self.siblings.iter().flatten().copied().collect()
    }

    pub fn deserialize(data: &[u8]) -> Result<Self, std::io::Error> {
        if data.len() % 32 != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid proof data length: {} (not a multiple of 32)", data.len()),
            ));
        }
        let siblings = data.chunks(32).map(|chunk| chunk.try_into().unwrap()).collect();
        Ok(Self { siblings })
    }

    /// Verify a Merkle proof against a known root hash.
    pub fn verify(&self, root: &MerkleHash, leaf_hash: &MerkleHash, leaf_index: usize) -> bool {
        let mut current_hash = *leaf_hash;
        let mut index = leaf_index;

        for sibling in &self.siblings {
            current_hash = if index % 2 == 0 {
                hash_pair(&current_hash, sibling)
            } else {
                hash_pair(sibling, &current_hash)
            };
            index /= 2;
        }

        current_hash == *root
    }
}

/// Build the Merkle tree from leaves, returning all nodes.
fn build_tree(leaves: &[MerkleHash]) -> Vec<MerkleHash> {
    if leaves.is_empty() {
        return vec![[0u8; 32]];
    }

    if leaves.len() == 1 {
        return vec![leaves[0]];
    }

    // Build tree again to collect all nodes
    let mut current_level = leaves.to_vec();
    let mut levels = vec![current_level.clone()];

    while current_level.len() > 1 {
        let mut next_level = Vec::new();

        for i in (0..current_level.len()).step_by(2) {
            let left = current_level[i];
            let right = if i + 1 < current_level.len() {
                current_level[i + 1]
            } else {
                // Odd number of nodes: duplicate the last one
                current_level[i]
            };

            next_level.push(hash_pair(&left, &right));
        }

        levels.push(next_level.clone());
        current_level = next_level;
    }

    // Flatten levels from top to bottom (root first)
    let mut result = Vec::new();
    for level in levels.iter().rev() {
        result.extend(level);
    }

    result
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_tree() {
        let tree = MerkleTree::new(&[]);
        assert_eq!(tree.leaf_count(), 0);
        assert_eq!(tree.root(), [0u8; 32]);
    }

    #[test]
    fn test_single_leaf() {
        let data = vec![vec![1, 2, 3, 4]];
        let tree = MerkleTree::new(&data);
        assert_eq!(tree.leaf_count(), 1);

        let proof = tree.prove(0).unwrap();
        assert_eq!(proof.siblings.len(), 0);
        assert!(tree.verify(&tree.leaves[0], &proof, 0));
    }

    #[test]
    fn test_two_leaves() {
        let data = vec![vec![1, 2, 3], vec![4, 5, 6]];
        let tree = MerkleTree::new(&data);
        assert_eq!(tree.leaf_count(), 2);

        // Test proof for first leaf
        let proof0 = tree.prove(0).unwrap();
        assert_eq!(proof0.siblings.len(), 1);
        assert!(tree.verify(&tree.leaves[0], &proof0, 0));

        // Test proof for second leaf
        let proof1 = tree.prove(1).unwrap();
        assert_eq!(proof1.siblings.len(), 1);
        assert!(tree.verify(&tree.leaves[1], &proof1, 1));
    }

    #[test]
    fn test_multiple_leaves() {
        let data: Vec<Vec<u8>> = (0..8).map(|i| vec![i]).collect();
        let tree = MerkleTree::new(&data);
        assert_eq!(tree.leaf_count(), 8);

        // Verify all leaves
        for i in 0..8 {
            let proof = tree.prove(i).unwrap();
            assert!(tree.verify(&tree.leaves[i], &proof, i));

            // Proof should have log2(8) = 3 siblings
            assert_eq!(proof.siblings.len(), 3);
        }
    }

    #[test]
    fn test_proof_verification_fails_for_wrong_leaf() {
        let data: Vec<Vec<u8>> = (0..4).map(|i| vec![i]).collect();
        let tree = MerkleTree::new(&data);

        let proof = tree.prove(0).unwrap();
        let wrong_leaf = tree.leaves[1];

        // Should fail with wrong leaf
        assert!(!tree.verify(&wrong_leaf, &proof, 0));
    }

    #[test]
    fn test_odd_number_of_leaves() {
        let data: Vec<Vec<u8>> = (0..7).map(|i| vec![i]).collect();
        let tree = MerkleTree::new(&data);
        assert_eq!(tree.leaf_count(), 7);

        // All proofs should still verify
        for i in 0..7 {
            let proof = tree.prove(i).unwrap();
            assert!(tree.verify(&tree.leaves[i], &proof, i));
        }
    }

    #[test]
    fn test_from_leaves() {
        let leaves: Vec<MerkleHash> = (0..4)
            .map(|i| {
                let mut hash = [0u8; 32];
                hash[0] = i;
                hash
            })
            .collect();

        let tree = MerkleTree::from_leaves(leaves.clone());
        assert_eq!(tree.leaf_count(), 4);

        for (i, leaf) in leaves.iter().enumerate() {
            let proof = tree.prove(i).unwrap();
            assert!(tree.verify(leaf, &proof, i));
        }
    }

    #[test]
    fn test_deterministic_root() {
        let data: Vec<Vec<u8>> = vec![vec![1, 2, 3], vec![4, 5, 6], vec![7, 8, 9]];

        let tree1 = MerkleTree::new(&data);
        let tree2 = MerkleTree::new(&data);

        assert_eq!(tree1.root(), tree2.root());
    }
}
