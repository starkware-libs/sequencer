//! Reproduces the proving side's recursive proof-tree digests over privacy transactions'
//! proof facts. Matches the goldens from the `proving` crate `stwo_run_and_prove_recursive_tree`.

use blake2::{Blake2s256, Digest};
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::Blake2Felt252;

#[cfg(test)]
#[path = "proof_fact_fold_test.rs"]
mod proof_fact_fold_test;

pub const BLAKE2S_DIGEST_N_WORDS: usize = 8;

/// A Blake2s-256 digest as little-endian u32 words.
pub type Blake2sDigestWords = [u32; BLAKE2S_DIGEST_N_WORDS];

pub const LEAF_VERIFIER_CIRCUIT_HASHES: [Blake2sDigestWords; 1] = [[
    0xd2d85a42, 0x79697b22, 0x3a41a061, 0x011cb393, 0x7a040ec9, 0x4508f4ca, 0x42239409, 0x60f3baea,
]];

pub const MULTIVERIFIER_CIRCUIT_HASH: Blake2sDigestWords = [
    0xa5989715, 0x2377c07a, 0xc6d1e844, 0x54f0a04d, 0x8be65a7d, 0xfd73c261, 0x9078e728, 0x973f680f,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FoldEntry {
    pub circuit_hash: Blake2sDigestWords,
    pub output_digest: Blake2sDigestWords,
}

#[derive(Clone, Copy, Debug)]
pub struct TransactionProofFacts<'a> {
    pub proof_facts: &'a [Felt],
    pub leaf_circuit_index: usize,
}

impl FoldEntry {
    fn to_words(self) -> Vec<u32> {
        self.circuit_hash.iter().chain(self.output_digest.iter()).copied().collect()
    }
}

pub fn single_transaction_root_entry(
    transaction_proof_facts: TransactionProofFacts<'_>,
) -> FoldEntry {
    let leaf_entry = FoldEntry {
        circuit_hash: LEAF_VERIFIER_CIRCUIT_HASHES[transaction_proof_facts.leaf_circuit_index],
        output_digest: compute_leaf_output_digest(transaction_proof_facts.proof_facts),
    };
    fold_pair(&leaf_entry, &leaf_entry)
}

pub fn compute_fold_digest(entry: &FoldEntry) -> Blake2sDigestWords {
    blake2s_over_u32_words(&entry.to_words())
}

/// Computes one transaction's leaf output digest:
/// blake2s(encode_felt252s_to_u32s(proof_facts[2..])). The preimage drops the two
/// version markers, keeping [program_hash, ...virtual OS output].
pub fn compute_leaf_output_digest(proof_facts: &[Felt]) -> Blake2sDigestWords {
    assert!(proof_facts.len() >= 3, "proof facts must contain at least 3 felts");
    blake2s_over_u32_words(&Blake2Felt252::encode_felts_to_u32s(&proof_facts[2..]))
}

pub fn blake2s_over_u32_words(words: &[u32]) -> Blake2sDigestWords {
    let bytes: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    let digest_bytes: [u8; 32] = Blake2s256::digest(&bytes).into();
    std::array::from_fn(|word_index| {
        u32::from_le_bytes(digest_bytes[word_index * 4..(word_index + 1) * 4].try_into().unwrap())
    })
}

fn fold_pair(left_entry: &FoldEntry, right_entry: &FoldEntry) -> FoldEntry {
    FoldEntry {
        circuit_hash: MULTIVERIFIER_CIRCUIT_HASH,
        output_digest: blake2s_over_u32_words(
            &[left_entry.to_words(), right_entry.to_words()].concat(),
        ),
    }
}
