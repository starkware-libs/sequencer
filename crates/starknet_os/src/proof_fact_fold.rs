//! Rust mirror of the OS proof-fact fold (`proof_fact_fold.cairo`): folds a block's
//! privacy-transaction proof facts into the recursive proof tree's digests, exactly as
//! the proving side (`stwo_run_and_prove_recursive_tree` and the multiverifier circuit)
//! computes them. The Cairo implementation is tested for bit-exact agreement with this
//! module, and this module is tested against golden values taken from real proven trees
//! on the proving side.

use blake2::{Blake2s256, Digest};
use starknet_types_core::felt::Felt;
use starknet_types_core::hash::Blake2Felt252;

#[cfg(test)]
#[path = "proof_fact_fold_test.rs"]
mod proof_fact_fold_test;

/// Number of u32 words in a Blake2s-256 digest.
pub const BLAKE2S_DIGEST_N_WORDS: usize = 8;

/// A Blake2s-256 digest as eight little-endian u32 words - the wire form of every digest
/// in the fold.
pub type Blake2sDigestWords = [u32; BLAKE2S_DIGEST_N_WORDS];

/// The leaf cairo-verifier circuit's hash:
/// blake2s(log_blowup_factor || component_log_sizes || preprocessed_root). A circuit
/// identity from the proving side's circuit registry; a change is a protocol version
/// change.
///
/// NOTE: this is the `canonical_small` test-registry value (proving side:
/// crates/stwo_run_and_prove_recursive_tree/test_data/circuit_registry.json), to be
/// replaced with the production registry's value before production use.
pub const LEAF_VERIFIER_CIRCUIT_HASH: Blake2sDigestWords = [
    0xd2d85a42, 0x79697b22, 0x3a41a061, 0x011cb393, 0x7a040ec9, 0x4508f4ca, 0x42239409, 0x60f3baea,
];

/// The multiverifier circuit's hash; see [`LEAF_VERIFIER_CIRCUIT_HASH`] for the format
/// and provenance caveat. A single value covers every fold node including the root: the
/// internal and root folds differ only in their Fiat-Shamir channel, not in the Merkle
/// hasher the circuit hash commits to.
pub const MULTIVERIFIER_CIRCUIT_HASH: Blake2sDigestWords = [
    0xa5989715, 0x2377c07a, 0xc6d1e844, 0x54f0a04d, 0x8be65a7d, 0xfd73c261, 0x9078e728, 0x973f680f,
];

/// A fold-tree entry: the hash of the circuit that proved this node, and the node's
/// output digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FoldEntry {
    pub circuit_hash: Blake2sDigestWords,
    pub output_digest: Blake2sDigestWords,
}

impl FoldEntry {
    /// The entry's 16 words in hash-preimage order: circuit hash, then output digest.
    fn to_words(&self) -> Vec<u32> {
        self.circuit_hash.iter().chain(self.output_digest.iter()).copied().collect()
    }
}

/// Folds the proof facts of a block's privacy transactions into the block's root entry.
/// Mirrors `fold_block_proof_facts` in `proof_fact_fold.cairo`; see the module
/// documentation there for the digest chain. Transactions with empty proof facts must
/// not be included.
///
/// # Panics
/// If `per_transaction_proof_facts` is empty (mirroring the proving side, which rejects
/// an empty leaf list - such a block must be omitted from any higher fold), or if any
/// transaction's proof facts are shorter than 3 felts.
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

/// Computes the digest the Cairo circuit verifier outputs for the proof whose facts fold
/// to `entry`: blake2s over the entry's 16 words (proving side:
/// `get_verification_output`).
pub fn compute_fold_digest(entry: &FoldEntry) -> Blake2sDigestWords {
    blake2s_over_u32_words(&entry.to_words())
}

/// Computes one transaction's leaf output digest:
/// blake2s(encode_felt252s_to_u32s(proof_facts[2..])). The preimage drops the proof
/// version and variant markers and keeps [program_hash, ...virtual OS output] - exactly
/// the preimage the leaf simple bootloader hashes to its own output, which the leaf
/// cairo-verifier circuit emits verbatim as its public output.
///
/// # Panics
/// If `proof_facts` is shorter than 3 felts (the two version markers plus the program
/// hash).
pub fn compute_leaf_output_digest(proof_facts: &[Felt]) -> Blake2sDigestWords {
    assert!(proof_facts.len() >= 3, "proof facts must contain at least 3 felts");
    blake2s_over_u32_words(&Blake2Felt252::encode_felts_to_u32s(&proof_facts[2..]))
}

/// Blake2s-256 over u32 words fed as little-endian bytes, with the 32-byte digest read
/// back as eight little-endian u32 words.
pub fn blake2s_over_u32_words(words: &[u32]) -> Blake2sDigestWords {
    let bytes: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    let digest_bytes: [u8; 32] = Blake2s256::digest(&bytes).into();
    std::array::from_fn(|word_index| {
        u32::from_le_bytes(digest_bytes[word_index * 4..(word_index + 1) * 4].try_into().unwrap())
    })
}

/// Folds layer-0 entries up to the tree's single root entry. A single entry self-folds
/// (the multiverifier verifies the same proof in both slots); with more, layers of
/// adjacent pairs are folded left to right, carrying a trailing unpaired entry unchanged,
/// until one entry remains (no self-fold at the end).
fn fold_entries_to_root(entries: Vec<FoldEntry>) -> FoldEntry {
    let mut current_layer_entries = entries;
    if current_layer_entries.len() == 1 {
        let single_entry = &current_layer_entries[0];
        return fold_pair(single_entry, single_entry);
    }
    while current_layer_entries.len() > 1 {
        let mut next_layer_entries = Vec::with_capacity(current_layer_entries.len().div_ceil(2));
        let mut entry_pairs = current_layer_entries.into_iter();
        while let Some(left_entry) = entry_pairs.next() {
            match entry_pairs.next() {
                Some(right_entry) => next_layer_entries.push(fold_pair(&left_entry, &right_entry)),
                None => next_layer_entries.push(left_entry),
            }
        }
        current_layer_entries = next_layer_entries;
    }
    current_layer_entries.pop().expect("the fold loop terminates with exactly one entry")
}

/// Folds two entries into their parent: the parent's output digest is blake2s over the
/// children's 32 raw u32 words (left entry then right entry, no felt encoding at fold
/// levels), and its circuit hash is the multiverifier's.
fn fold_pair(left_entry: &FoldEntry, right_entry: &FoldEntry) -> FoldEntry {
    let children_words: Vec<u32> =
        left_entry.to_words().into_iter().chain(right_entry.to_words()).collect();
    FoldEntry {
        circuit_hash: MULTIVERIFIER_CIRCUIT_HASH,
        output_digest: blake2s_over_u32_words(&children_words),
    }
}
