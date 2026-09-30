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

// The circuit hashes are the leaf verifier (at trace log size 20) and multiverifier entries of the
// proving repo's canonical_small circuit registry,
// `crates/stwo_run_and_prove_recursive_tree/test_data/circuit_registry.json` at the pinned proving
// rev (`2b495a36`). The same values are in the Cairo `proof_fact_fold`.
// TODO(Einat): the constants are all canonical_small test values; replace them with the production
// registry values once it is generated.
// TODO(Einat): import the circuit hashes from the proving repo's registry once it is exposed from a
// crate, instead of copying them.
// TODO(Einat): accept proofs from the leaf verifiers of all the registry's trace sizes, not only
// one. The multiverifier outputs each leaf's circuit hash unchecked, so the OS must check it
// against the registry's list of leaf verifier circuit hashes.
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

pub fn blake2s_over_u32_words(words: &[u32]) -> Blake2sDigestWords {
    let bytes: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    let digest_bytes: [u8; 32] = Blake2s256::digest(&bytes).into();
    std::array::from_fn(|word_index| {
        u32::from_le_bytes(digest_bytes[word_index * 4..(word_index + 1) * 4].try_into().unwrap())
    })
}
