// Folds a block's privacy-transaction proof facts into a single Blake2s digest - the OS
// mirror of the recursive proof tree the proving side builds over the corresponding leaf
// proofs (proving side: `stwo_run_and_prove_recursive_tree`). The tree's root proof is
// checked by the Cairo circuit verifier, whose public output is a digest of the folded
// facts; the fold here must therefore reproduce the proving side's digest chain bit for
// bit, so that the two can be compared.
//
// The digest chain:
// * A fold-tree entry is 16 u32 words: a circuit hash (8 words) followed by an output
//   digest (8 words). All digests are Blake2s-256, and every 32-byte digest is read as
//   eight little-endian u32 words.
// * Leaf (one privacy transaction): the output digest is
//   blake2s(encode_felt252s_to_u32s(proof_facts[2:])). The preimage drops
//   proof_facts[0] (the proof version) and proof_facts[1] (the proof variant marker) and
//   keeps [program_hash, ...virtual OS output] - exactly the preimage the leaf simple
//   bootloader hashes to its own output. The entry's circuit hash is the leaf
//   cairo-verifier circuit's.
// * Internal node: adjacent entries are paired left to right; a parent's output digest
//   is blake2s over the 32 raw u32 words of its two children's entries (no felt
//   encoding at fold levels), and its circuit hash is the multiverifier circuit's. A
//   trailing unpaired entry is carried to the next layer unchanged, keeping its own
//   circuit hash.
// * A block with a single entry self-folds: the root preimage is that entry in both
//   child slots (the multiverifier verifies the same proof twice). The root is therefore
//   always a multiverifier node.
// * The digest the circuit verifier outputs for a proof whose facts fold to some entry
//   is blake2s over that entry's 16 words (circuit hash, then output digest).
//
// The fold arity is pinned to 2; a change on the proving side is a protocol change that
// must be mirrored here.

from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.cairo_blake2s.blake2s import blake_with_opcode, encode_felt252s_to_u32s
from starkware.cairo.common.math import assert_not_zero
from starkware.cairo.common.memcpy import memcpy

// Number of u32 words in a Blake2s-256 digest.
const BLAKE2S_DIGEST_N_WORDS = 8;
// A fold-tree entry: a circuit hash (8 words) followed by an output digest (8 words).
const FOLD_ENTRY_N_WORDS = 2 * BLAKE2S_DIGEST_N_WORDS;

// The circuit hashes identifying the leaf cairo-verifier circuit and the multiverifier
// circuit: blake2s(log_blowup_factor || component_log_sizes || preprocessed_root), as
// eight little-endian u32 words. These are circuit identities from the proving side's
// circuit registry and are not derivable from the proof facts; a change to either is a
// protocol version change.
//
// NOTE: these are the `canonical_small` test-registry values (proving side:
// crates/stwo_run_and_prove_recursive_tree/test_data/circuit_registry.json), pinned by
// tests against the same values in the Rust mirror (starknet_os::proof_fact_fold). They
// must be replaced with the production registry's values before production use.
const LEAF_VERIFIER_CIRCUIT_HASH_0 = 0xd2d85a42;
const LEAF_VERIFIER_CIRCUIT_HASH_1 = 0x79697b22;
const LEAF_VERIFIER_CIRCUIT_HASH_2 = 0x3a41a061;
const LEAF_VERIFIER_CIRCUIT_HASH_3 = 0x011cb393;
const LEAF_VERIFIER_CIRCUIT_HASH_4 = 0x7a040ec9;
const LEAF_VERIFIER_CIRCUIT_HASH_5 = 0x4508f4ca;
const LEAF_VERIFIER_CIRCUIT_HASH_6 = 0x42239409;
const LEAF_VERIFIER_CIRCUIT_HASH_7 = 0x60f3baea;

const MULTIVERIFIER_CIRCUIT_HASH_0 = 0xa5989715;
const MULTIVERIFIER_CIRCUIT_HASH_1 = 0x2377c07a;
const MULTIVERIFIER_CIRCUIT_HASH_2 = 0xc6d1e844;
const MULTIVERIFIER_CIRCUIT_HASH_3 = 0x54f0a04d;
const MULTIVERIFIER_CIRCUIT_HASH_4 = 0x8be65a7d;
const MULTIVERIFIER_CIRCUIT_HASH_5 = 0xfd73c261;
const MULTIVERIFIER_CIRCUIT_HASH_6 = 0x9078e728;
const MULTIVERIFIER_CIRCUIT_HASH_7 = 0x973f680f;

// Folds the proof facts of a block's privacy transactions into the block's root entry:
// [multiverifier circuit hash (8 words), root output digest (8 words)].
//
// `proof_facts_sizes[i]` is the length of the i'th transaction's proof facts, and the
// facts themselves are laid out consecutively in `concatenated_proof_facts`.
// Preconditions: `n_transactions` is at least 1 (a block with no contributing
// transactions has no fold and must be omitted by the caller); every size is at least 3
// (the two version markers plus the program hash), as enforced for non-empty proof facts
// by `check_proof_facts`; transactions with empty proof facts must not be included.
func fold_block_proof_facts{range_check_ptr}(
    n_transactions: felt, proof_facts_sizes: felt*, concatenated_proof_facts: felt*
) -> (root_entry: felt*) {
    alloc_locals;
    assert_not_zero(n_transactions);
    let (local leaf_entries: felt*) = alloc();
    build_leaf_entries(
        n_transactions=n_transactions,
        proof_facts_sizes=proof_facts_sizes,
        concatenated_proof_facts=concatenated_proof_facts,
        leaf_entries=leaf_entries,
    );
    return fold_entries_to_root(n_entries=n_transactions, entries=leaf_entries);
}

// Computes the digest the Cairo circuit verifier outputs for the proof whose facts fold
// to `entry`: blake2s over the entry's 16 words (proving side:
// `get_verification_output`). Returns the digest as 8 u32 words.
func compute_fold_digest{range_check_ptr}(entry: felt*) -> (fold_digest: felt*) {
    alloc_locals;
    let (local fold_digest: felt*) = alloc();
    blake_with_opcode(len=FOLD_ENTRY_N_WORDS, data=entry, out=fold_digest);
    return (fold_digest=fold_digest);
}

// Computes one transaction's leaf output digest:
// blake2s(encode_felt252s_to_u32s(proof_facts[2:])), as 8 u32 words. This is the digest
// the leaf simple bootloader writes to its output, which the leaf cairo-verifier circuit
// emits verbatim as its public output.
// Precondition: `proof_facts_size` is at least 3.
func compute_leaf_output_digest{range_check_ptr}(proof_facts_size: felt, proof_facts: felt*) -> (
    output_digest: felt*
) {
    alloc_locals;
    let (local encoded_words: felt*) = alloc();
    let encoded_words_len = encode_felt252s_to_u32s(
        packed_values_len=proof_facts_size - 2,
        packed_values=proof_facts + 2,
        unpacked_u32s=encoded_words,
    );
    let (local output_digest: felt*) = alloc();
    blake_with_opcode(len=encoded_words_len, data=encoded_words, out=output_digest);
    return (output_digest=output_digest);
}

// Returns the leaf cairo-verifier circuit's hash as 8 u32 words.
func get_leaf_verifier_circuit_hash() -> (circuit_hash: felt*) {
    let (circuit_hash: felt*) = alloc();
    assert circuit_hash[0] = LEAF_VERIFIER_CIRCUIT_HASH_0;
    assert circuit_hash[1] = LEAF_VERIFIER_CIRCUIT_HASH_1;
    assert circuit_hash[2] = LEAF_VERIFIER_CIRCUIT_HASH_2;
    assert circuit_hash[3] = LEAF_VERIFIER_CIRCUIT_HASH_3;
    assert circuit_hash[4] = LEAF_VERIFIER_CIRCUIT_HASH_4;
    assert circuit_hash[5] = LEAF_VERIFIER_CIRCUIT_HASH_5;
    assert circuit_hash[6] = LEAF_VERIFIER_CIRCUIT_HASH_6;
    assert circuit_hash[7] = LEAF_VERIFIER_CIRCUIT_HASH_7;
    return (circuit_hash=circuit_hash);
}

// Returns the multiverifier circuit's hash as 8 u32 words.
func get_multiverifier_circuit_hash() -> (circuit_hash: felt*) {
    let (circuit_hash: felt*) = alloc();
    assert circuit_hash[0] = MULTIVERIFIER_CIRCUIT_HASH_0;
    assert circuit_hash[1] = MULTIVERIFIER_CIRCUIT_HASH_1;
    assert circuit_hash[2] = MULTIVERIFIER_CIRCUIT_HASH_2;
    assert circuit_hash[3] = MULTIVERIFIER_CIRCUIT_HASH_3;
    assert circuit_hash[4] = MULTIVERIFIER_CIRCUIT_HASH_4;
    assert circuit_hash[5] = MULTIVERIFIER_CIRCUIT_HASH_5;
    assert circuit_hash[6] = MULTIVERIFIER_CIRCUIT_HASH_6;
    assert circuit_hash[7] = MULTIVERIFIER_CIRCUIT_HASH_7;
    return (circuit_hash=circuit_hash);
}

// Builds the layer-0 fold entries, one per transaction, consecutively at `leaf_entries`.
func build_leaf_entries{range_check_ptr}(
    n_transactions: felt,
    proof_facts_sizes: felt*,
    concatenated_proof_facts: felt*,
    leaf_entries: felt*,
) {
    alloc_locals;
    if (n_transactions == 0) {
        return ();
    }
    let (leaf_verifier_circuit_hash) = get_leaf_verifier_circuit_hash();
    memcpy(dst=leaf_entries, src=leaf_verifier_circuit_hash, len=BLAKE2S_DIGEST_N_WORDS);
    let (output_digest) = compute_leaf_output_digest(
        proof_facts_size=proof_facts_sizes[0], proof_facts=concatenated_proof_facts
    );
    memcpy(
        dst=leaf_entries + BLAKE2S_DIGEST_N_WORDS, src=output_digest, len=BLAKE2S_DIGEST_N_WORDS
    );
    return build_leaf_entries(
        n_transactions=n_transactions - 1,
        proof_facts_sizes=proof_facts_sizes + 1,
        concatenated_proof_facts=concatenated_proof_facts + proof_facts_sizes[0],
        leaf_entries=leaf_entries + FOLD_ENTRY_N_WORDS,
    );
}

// Folds `n_entries` consecutive entries at `entries` up to the tree's single root entry.
// Precondition: `n_entries` is at least 1. A single layer-0 entry self-folds; with more,
// layers of adjacent pairs are folded until one entry remains (no self-fold at the end).
func fold_entries_to_root{range_check_ptr}(n_entries: felt, entries: felt*) -> (root_entry: felt*) {
    alloc_locals;
    if (n_entries == 1) {
        // Self-fold: the root preimage is the single entry in both child slots.
        let (local self_fold_preimage: felt*) = alloc();
        memcpy(dst=self_fold_preimage, src=entries, len=FOLD_ENTRY_N_WORDS);
        memcpy(dst=self_fold_preimage + FOLD_ENTRY_N_WORDS, src=entries, len=FOLD_ENTRY_N_WORDS);
        let (local root_entry: felt*) = alloc();
        fold_pair(children=self_fold_preimage, parent_entry=root_entry);
        return (root_entry=root_entry);
    }
    return fold_layers_to_root(n_entries=n_entries, entries=entries);
}

// Folds layers of adjacent pairs until a single entry remains and returns it.
// Precondition: `n_entries` is at least 2 (a lone layer-0 entry self-folds instead - see
// `fold_entries_to_root`).
func fold_layers_to_root{range_check_ptr}(n_entries: felt, entries: felt*) -> (root_entry: felt*) {
    alloc_locals;
    let (local next_layer_entries: felt*) = alloc();
    let (n_next_layer_entries) = fold_layer(
        n_entries=n_entries, entries=entries, next_layer_entries=next_layer_entries
    );
    if (n_next_layer_entries == 1) {
        return (root_entry=next_layer_entries);
    }
    return fold_layers_to_root(n_entries=n_next_layer_entries, entries=next_layer_entries);
}

// Folds one layer: pairs adjacent entries left to right into parent entries written
// consecutively at `next_layer_entries`, carrying a trailing unpaired entry unchanged.
// Returns the number of entries written to the next layer.
func fold_layer{range_check_ptr}(n_entries: felt, entries: felt*, next_layer_entries: felt*) -> (
    n_next_layer_entries: felt
) {
    if (n_entries == 0) {
        return (n_next_layer_entries=0);
    }
    if (n_entries == 1) {
        // Carry the unpaired entry unchanged, keeping its own circuit hash.
        memcpy(dst=next_layer_entries, src=entries, len=FOLD_ENTRY_N_WORDS);
        return (n_next_layer_entries=1);
    }
    fold_pair(children=entries, parent_entry=next_layer_entries);
    let (n_rest_entries) = fold_layer(
        n_entries=n_entries - 2,
        entries=entries + 2 * FOLD_ENTRY_N_WORDS,
        next_layer_entries=next_layer_entries + FOLD_ENTRY_N_WORDS,
    );
    return (n_next_layer_entries=n_rest_entries + 1);
}

// Folds the two consecutive entries at `children` into `parent_entry`: the parent's
// output digest is blake2s over the children's 32 raw u32 words - the two consecutive
// entries are exactly that preimage - and its circuit hash is the multiverifier's.
func fold_pair{range_check_ptr}(children: felt*, parent_entry: felt*) {
    alloc_locals;
    let (multiverifier_circuit_hash) = get_multiverifier_circuit_hash();
    memcpy(dst=parent_entry, src=multiverifier_circuit_hash, len=BLAKE2S_DIGEST_N_WORDS);
    blake_with_opcode(
        len=2 * FOLD_ENTRY_N_WORDS, data=children, out=parent_entry + BLAKE2S_DIGEST_N_WORDS
    );
    return ();
}
