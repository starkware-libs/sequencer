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

// TODO(Einat): the circuit hashes below, the vendored registry, the verifier executable and the
// processed-proof fixture are all canonical_small test values; replace them with the production
// registry values once it is generated.
pub const LEAF_VERIFIER_CIRCUIT_HASH: Blake2sDigestWords = [
    0xd2d85a42, 0x79697b22, 0x3a41a061, 0x011cb393, 0x7a040ec9, 0x4508f4ca, 0x42239409, 0x60f3baea,
];

pub const MULTIVERIFIER_CIRCUIT_HASH: Blake2sDigestWords = [
    0xa5989715, 0x2377c07a, 0xc6d1e844, 0x54f0a04d, 0x8be65a7d, 0xfd73c261, 0x9078e728, 0x973f680f,
];

/// Computes the output digest of a processed proof covering two verified proofs. The proving side
/// processes proofs into a proof of the multiverifier circuit, whose output packs the circuit hash
/// and output digest of each proof verified in its two verifier slots. A single transaction's
/// proof fills both slots, so its callers pass the same proof facts twice.
pub fn compute_processed_proof_output_digest(
    first_proof_facts: &[Felt],
    second_proof_facts: &[Felt],
) -> Blake2sDigestWords {
    let proof_entry_words = |proof_facts: &[Felt]| -> Vec<u32> {
        LEAF_VERIFIER_CIRCUIT_HASH
            .into_iter()
            .chain(compute_leaf_output_digest(proof_facts))
            .collect()
    };
    blake2s_over_u32_words(
        &[proof_entry_words(first_proof_facts), proof_entry_words(second_proof_facts)].concat(),
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
