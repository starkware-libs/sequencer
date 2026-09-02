//! Reproduces the proving side's recursive proof-tree digests over privacy transactions'
//! proof facts, tested against goldens from real proven trees.

use blake2::{Blake2s256, Digest};
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::Blake2Felt252;

#[cfg(test)]
#[path = "proof_fact_fold_test.rs"]
mod proof_fact_fold_test;

pub const BLAKE2S_DIGEST_N_WORDS: usize = 8;

/// A Blake2s-256 digest as little-endian u32 words.
pub type Blake2sDigestWords = [u32; BLAKE2S_DIGEST_N_WORDS];

/// The leaf cairo-verifier circuit's hash, from the proving side's circuit registry:
/// blake2s(log_blowup_factor || component_log_sizes || preprocessed_root).
///
/// NOTE: `canonical_small` test-registry value, pinned against the vendored
/// `resources/circuit_registry_canonical_small.json`; to be replaced - together with the
/// registry - with the production value before production use.
pub const LEAF_VERIFIER_CIRCUIT_HASH: Blake2sDigestWords = [
    0xd2d85a42, 0x79697b22, 0x3a41a061, 0x011cb393, 0x7a040ec9, 0x4508f4ca, 0x42239409, 0x60f3baea,
];

/// The multiverifier circuit's hash (same format and caveat as
/// [`LEAF_VERIFIER_CIRCUIT_HASH`]). One value covers all fold nodes: internal and root
/// folds differ only in their Fiat-Shamir channel.
pub const MULTIVERIFIER_CIRCUIT_HASH: Blake2sDigestWords = [
    0xa5989715, 0x2377c07a, 0xc6d1e844, 0x54f0a04d, 0x8be65a7d, 0xfd73c261, 0x9078e728, 0x973f680f,
];

/// A fold-tree node: the hash of the circuit that proved it, and its output digest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FoldEntry {
    pub circuit_hash: Blake2sDigestWords,
    pub output_digest: Blake2sDigestWords,
}

impl FoldEntry {
    fn to_words(&self) -> Vec<u32> {
        self.circuit_hash.iter().chain(self.output_digest.iter()).copied().collect()
    }
}

/// Folds a block's privacy transactions' proof facts into the block's root entry.
/// Transactions with empty proof facts must not be included.
///
/// # Panics
/// If `per_transaction_proof_facts` is empty (the proving side rejects an empty leaf
/// list) or any transaction's proof facts are shorter than 3 felts.
pub fn fold_block_proof_facts(per_transaction_proof_facts: &[&[Felt]]) -> FoldEntry {
    assert!(
        !per_transaction_proof_facts.is_empty(),
        "a block fold requires at least one transaction's proof facts"
    );
    let leaf_entries: Vec<FoldEntry> = per_transaction_proof_facts
        .iter()
        .map(|proof_facts| FoldEntry {
            circuit_hash: LEAF_VERIFIER_CIRCUIT_HASH,
            output_digest: compute_leaf_output_digest(proof_facts),
        })
        .collect();
    fold_entries_to_root(leaf_entries)
}

/// Folds block-root entries (each a multiverifier node produced by
/// [`fold_block_proof_facts`]) into a single entry, for combining blocks. Unlike a
/// block's own fold, a single entry is returned unchanged - it is a carried subtree
/// root, not self-folded; self-folding happens only at a block's leaf layer.
///
/// # Panics
/// If `entries` is empty.
pub fn fold_block_root_entries(entries: Vec<FoldEntry>) -> FoldEntry {
    assert!(!entries.is_empty(), "folding block-root entries requires at least one entry");
    fold_layers_to_root(entries)
}

/// The digest the circuit verifier outputs for the proof whose facts fold to `entry`:
/// blake2s over its 16 words (proving side: `get_verification_output`).
pub fn compute_fold_digest(entry: &FoldEntry) -> Blake2sDigestWords {
    blake2s_over_u32_words(&entry.to_words())
}

/// Packs an output digest into the (low, high) felt pair used in the OS output: the same
/// Uint256 composition `encode_felt252_data_and_calc_blake2s` uses - low and high are
/// the little-endian word compositions of words 0-3 and 4-7.
pub fn pack_output_digest(output_digest: &Blake2sDigestWords) -> (Felt, Felt) {
    let pack_half = |words: &[u32]| {
        words.iter().rev().fold(Felt::ZERO, |packed_half, word| {
            packed_half * Felt::from(1u64 << 32) + Felt::from(*word)
        })
    };
    (pack_half(&output_digest[..4]), pack_half(&output_digest[4..]))
}

/// Computes one transaction's leaf output digest:
/// blake2s(encode_felt252s_to_u32s(proof_facts[2..])). The preimage drops the two
/// version markers, keeping [program_hash, ...virtual OS output] - the preimage the
/// proving side's leaf verifier circuit emits as its output digest.
///
/// # Panics
/// If `proof_facts` is shorter than 3 felts (the two markers plus the program hash).
pub fn compute_leaf_output_digest(proof_facts: &[Felt]) -> Blake2sDigestWords {
    assert!(proof_facts.len() >= 3, "proof facts must contain at least 3 felts");
    blake2s_over_u32_words(&Blake2Felt252::encode_felts_to_u32s(&proof_facts[2..]))
}

/// Blake2s-256 over the words' little-endian bytes, read back as little-endian u32 words.
pub fn blake2s_over_u32_words(words: &[u32]) -> Blake2sDigestWords {
    let bytes: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    let digest_bytes: [u8; 32] = Blake2s256::digest(&bytes).into();
    std::array::from_fn(|word_index| {
        u32::from_le_bytes(digest_bytes[word_index * 4..(word_index + 1) * 4].try_into().unwrap())
    })
}

/// Folds layer-0 entries into the single root entry. A single entry self-folds (the
/// multiverifier verifies the same proof in both slots).
fn fold_entries_to_root(entries: Vec<FoldEntry>) -> FoldEntry {
    match entries.as_slice() {
        [single_entry] => fold_pair(single_entry, single_entry),
        _ => fold_layers_to_root(entries),
    }
}

/// Folds layers of adjacent pairs left to right, carrying a trailing unpaired entry
/// unchanged, until one entry remains; a single entry is returned unchanged.
fn fold_layers_to_root(mut layer_entries: Vec<FoldEntry>) -> FoldEntry {
    while layer_entries.len() > 1 {
        layer_entries = layer_entries
            .chunks(2)
            .map(|entry_pair| match entry_pair {
                [left_entry, right_entry] => fold_pair(left_entry, right_entry),
                [lone_entry] => *lone_entry,
                _ => unreachable!("chunks(2) yields chunks of one or two entries"),
            })
            .collect();
    }
    layer_entries.pop().expect("the fold loop terminates with exactly one entry")
}

/// The parent's output digest is blake2s over the children's 32 raw u32 words (no felt
/// encoding at fold levels); its circuit hash is the multiverifier's.
fn fold_pair(left_entry: &FoldEntry, right_entry: &FoldEntry) -> FoldEntry {
    FoldEntry {
        circuit_hash: MULTIVERIFIER_CIRCUIT_HASH,
        output_digest: blake2s_over_u32_words(
            &[left_entry.to_words(), right_entry.to_words()].concat(),
        ),
    }
}
