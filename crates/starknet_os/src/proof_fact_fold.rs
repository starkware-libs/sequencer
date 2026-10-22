//! Reproduces the proving side's recursive proof-tree digests over privacy transactions'
//! proof facts. Matches the goldens from the `proving` crate `stwo_run_and_prove_recursive_tree`.

use blake2::{Blake2s256, Digest};
use leaf_proof_format::PackedNode;
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::Blake2Felt252;

#[cfg(test)]
#[path = "proof_fact_fold_test.rs"]
mod proof_fact_fold_test;

pub const BLAKE2S_DIGEST_N_WORDS: usize = 8;

/// A Blake2s-256 digest as little-endian u32 words.
pub type Blake2sDigestWords = [u32; BLAKE2S_DIGEST_N_WORDS];

// TODO(Einat): the constants are all canonical_small test values; replace them with the production
// registry values once it is generated.
// TODO(Einat): import the circuit hashes from the proving repo's registry once it is exposed from a
// crate, instead of copying them.
// TODO(Einat): accept proofs from the leaf verifiers of all the registry's trace sizes, not only
// one.
pub const LEAF_VERIFIER_CIRCUIT_HASH: Blake2sDigestWords = [
    0xd2d85a42, 0x79697b22, 0x3a41a061, 0x011cb393, 0x7a040ec9, 0x4508f4ca, 0x42239409, 0x60f3baea,
];

pub const MULTIVERIFIER_CIRCUIT_HASH: Blake2sDigestWords = [
    0xa5989715, 0x2377c07a, 0xc6d1e844, 0x54f0a04d, 0x8be65a7d, 0xfd73c261, 0x9078e728, 0x973f680f,
];

/// Computes one transaction's leaf output digest:
/// blake2s(encode_felt252s_to_u32s(proof_facts[2..])). The preimage drops the two
/// version markers, keeping [program_hash, ...virtual OS output].
pub fn compute_leaf_output_digest(proof_facts: &[Felt]) -> Blake2sDigestWords {
    assert!(proof_facts.len() >= 3, "proof facts must contain at least 3 felts");
    blake2s_over_u32_words(&Blake2Felt252::encode_felts_to_u32s(&proof_facts[2..]))
}

/// Combines two leaf digests into their parent's output digest.
pub fn combine_leaf_digests(
    proof_facts_left_leaf_digest: &Blake2sDigestWords,
    proof_facts_right_leaf_digest: &Blake2sDigestWords,
) -> Blake2sDigestWords {
    let left_proof_entry_words: Vec<u32> =
        LEAF_VERIFIER_CIRCUIT_HASH.into_iter().chain(*proof_facts_left_leaf_digest).collect();

    let right_proof_entry_words: Vec<u32> =
        LEAF_VERIFIER_CIRCUIT_HASH.into_iter().chain(*proof_facts_right_leaf_digest).collect();

    blake2s_over_u32_words(
        &[left_proof_entry_words.as_slice(), right_proof_entry_words.as_slice()].concat(),
    )
}

/// Computes the digest the circuit verifier outputs when run on the transaction's processed
/// proof: blake2s(multiverifier circuit hash || the processed proof's output digest).
pub fn compute_verification_digest(
    processed_proof_output_digest: &Blake2sDigestWords,
) -> Blake2sDigestWords {
    blake2s_over_u32_words(
        &[MULTIVERIFIER_CIRCUIT_HASH.as_slice(), processed_proof_output_digest.as_slice()].concat(),
    )
}

/// A node of the proving side's proof tree that is neither a transaction's proof nor a
/// multiverifier proof of one or two children.
#[derive(Debug, thiserror::Error)]
#[error("Unsupported proof facts tree node: {0}.")]
pub struct UnsupportedProofFactsTreeNode(pub String);

/// Flattens the proving side's tree of the proofs of transactions' proof facts (`packed_output` of
/// `stwo_run_and_prove_recursive_tree`) into the nodes the Cairo `write_proof_tree_root_entry`
/// guesses, in order: one entry per node, FLAT_TREE_FOLD (1) for a multiverifier proof of two
/// children and FLAT_TREE_LEAF (0) for a transaction's proof, each fold followed by its left
/// subtree, then its right subtree. The proving side's single-leaf tree, whose root multiverifier
/// verifies the leaf in both of its slots, is written as the leaf.
pub fn flatten_proof_facts_tree(
    packed_tree: &PackedNode,
) -> Result<Vec<Felt>, UnsupportedProofFactsTreeNode> {
    let tree = match packed_tree {
        PackedNode::Composite { subtasks, .. } => match subtasks.as_slice() {
            [leaf @ PackedNode::Composite { subtasks: leaf_subtasks, .. }]
                if matches!(leaf_subtasks.as_slice(), [PackedNode::Plain { .. }]) =>
            {
                leaf
            }
            _ => packed_tree,
        },
        PackedNode::Plain { .. } => packed_tree,
    };
    let mut flat_tree = Vec::new();
    write_flat_subtree(tree, &mut flat_tree)?;
    Ok(flat_tree)
}

pub fn blake2s_over_u32_words(words: &[u32]) -> Blake2sDigestWords {
    let bytes: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    let digest_bytes: [u8; 32] = Blake2s256::digest(&bytes).into();
    std::array::from_fn(|word_index| {
        u32::from_le_bytes(digest_bytes[word_index * 4..(word_index + 1) * 4].try_into().unwrap())
    })
}

fn write_flat_subtree(
    node: &PackedNode,
    flat_tree: &mut Vec<Felt>,
) -> Result<(), UnsupportedProofFactsTreeNode> {
    let unsupported_node = || UnsupportedProofFactsTreeNode(format!("{node:?}"));
    let PackedNode::Composite { subtasks, .. } = node else {
        return Err(unsupported_node());
    };
    match subtasks.as_slice() {
        [PackedNode::Plain { .. }] => flat_tree.push(Felt::ZERO),
        [left, right] => {
            flat_tree.push(Felt::ONE);
            write_flat_subtree(left, flat_tree)?;
            write_flat_subtree(right, flat_tree)?;
        }
        _ => return Err(unsupported_node()),
    }
    Ok(())
}
