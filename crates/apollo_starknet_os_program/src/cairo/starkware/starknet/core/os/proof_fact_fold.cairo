// The OS mirror of the recursive proof tree the proving side builds over privacy
// transactions' proof facts (`stwo_run_and_prove_recursive_tree`): reproduces the tree's
// digests bit for bit, so they can be compared with the circuit verifier's public output.

from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.cairo_blake2s.blake2s import blake_with_opcode, encode_felt252s_to_u32s
from starkware.cairo.common.math import assert_not_zero, unsigned_div_rem
from starkware.cairo.common.memcpy import memcpy
from starkware.cairo.common.registers import get_label_location

const BLAKE2S_DIGEST_N_WORDS = 8;
// A fold-tree entry: a circuit hash (8 words) followed by an output digest (8 words).
const FOLD_ENTRY_N_WORDS = 2 * BLAKE2S_DIGEST_N_WORDS;

// A reference to one transaction's proof facts, recorded during transaction execution.
struct ProofFactsReference {
    proof_facts_size: felt,
    proof_facts: felt*,
}

// Records a reference to an executed transaction's proof facts, to be folded at the end
// of the block; no-op for a transaction with empty proof facts.
func record_proof_facts_reference{proof_facts_references: ProofFactsReference*}(
    proof_facts_size: felt, proof_facts: felt*
) {
    if (proof_facts_size == 0) {
        return ();
    }
    assert [proof_facts_references] = ProofFactsReference(
        proof_facts_size=proof_facts_size, proof_facts=proof_facts
    );
    let proof_facts_references = &proof_facts_references[1];
    return ();
}

// Folds the proof-fact references recorded during a block's execution (the segment
// [proof_facts_references_start, proof_facts_references_end)) into the block's packed
// root output digest. Returns zeros for a block with no contributing transactions -
// such a block has no fold-tree node and must be skipped by any higher-level fold.
func fold_recorded_proof_facts{range_check_ptr}(
    proof_facts_references_start: ProofFactsReference*,
    proof_facts_references_end: ProofFactsReference*,
) -> (n_proof_facts_transactions: felt, root_output_low: felt, root_output_high: felt) {
    alloc_locals;
    local n_proof_facts_transactions = (proof_facts_references_end - proof_facts_references_start) /
        ProofFactsReference.SIZE;
    if (n_proof_facts_transactions == 0) {
        return (n_proof_facts_transactions=0, root_output_low=0, root_output_high=0);
    }
    let (root_entry) = fold_block_proof_facts(
        n_transactions=n_proof_facts_transactions,
        proof_facts_references=proof_facts_references_start,
    );
    let (root_output_low, root_output_high) = pack_output_digest(entry=root_entry);
    return (
        n_proof_facts_transactions=n_proof_facts_transactions,
        root_output_low=root_output_low,
        root_output_high=root_output_high,
    );
}

// Folds the proof facts of a block's privacy transactions into the block's root entry:
// [multiverifier circuit hash, root output digest].
// Preconditions: `n_transactions` is at least 1 and every referenced proof facts size is
// at least 3; transactions with empty proof facts must not be included.
func fold_block_proof_facts{range_check_ptr}(
    n_transactions: felt, proof_facts_references: ProofFactsReference*
) -> (root_entry: felt*) {
    alloc_locals;
    assert_not_zero(n_transactions);
    let (local leaf_entries: felt*) = alloc();
    build_leaf_entries(
        n_transactions=n_transactions,
        proof_facts_references=proof_facts_references,
        leaf_entries=leaf_entries,
    );
    return fold_entries_to_root(n_entries=n_transactions, entries=leaf_entries);
}

// Folds `n_entries` consecutive block-root entries (each a multiverifier node produced
// by `fold_block_proof_facts`) into a single entry, for combining blocks. Unlike a
// block's own fold, a single entry is returned unchanged - it is a carried subtree root,
// not self-folded; self-folding happens only at a block's leaf layer.
// Precondition: `n_entries` is at least 1.
func fold_block_root_entries{range_check_ptr}(n_entries: felt, entries: felt*) -> (
    root_entry: felt*
) {
    if (n_entries == 1) {
        return (root_entry=entries);
    }
    return fold_layers_to_root(n_entries=n_entries, entries=entries);
}

// Packs an entry's output digest (eight u32 words) into the (low, high) pair used in the
// OS output: the same Uint256 composition `encode_felt252_data_and_calc_blake2s` uses -
// low and high are the little-endian word compositions of words 0-3 and 4-7.
// Assumption: each word is in [0, 2^32) (as blake opcode outputs are).
func pack_output_digest(entry: felt*) -> (low: felt, high: felt) {
    let output_digest = entry + BLAKE2S_DIGEST_N_WORDS;
    return (
        low=output_digest[0] + output_digest[1] * (2 ** 32) + output_digest[2] * (2 ** 64) +
        output_digest[3] * (2 ** 96),
        high=output_digest[4] + output_digest[5] * (2 ** 32) + output_digest[6] * (2 ** 64) +
        output_digest[7] * (2 ** 96),
    );
}

// The inverse of `pack_output_digest`: returns a block-root entry - the multiverifier
// circuit hash followed by the unpacked output digest words - range-checking that `low`
// and `high` are under 2^128 and every unpacked word is a u32.
func unpack_block_root_entry{range_check_ptr}(low: felt, high: felt) -> (entry: felt*) {
    alloc_locals;
    let (local entry: felt*) = alloc();
    let (multiverifier_circuit_hash) = get_multiverifier_circuit_hash();
    memcpy(dst=entry, src=multiverifier_circuit_hash, len=BLAKE2S_DIGEST_N_WORDS);
    unpack_digest_half(packed_half=low, digest_words_out=entry + BLAKE2S_DIGEST_N_WORDS);
    unpack_digest_half(packed_half=high, digest_words_out=entry + BLAKE2S_DIGEST_N_WORDS + 4);
    return (entry=entry);
}

// The digest the circuit verifier outputs for the proof whose facts fold to `entry`:
// blake2s over the entry's 16 words (proving side: `get_verification_output`).
func compute_fold_digest{range_check_ptr}(entry: felt*) -> (fold_digest: felt*) {
    alloc_locals;
    let (local fold_digest: felt*) = alloc();
    blake_with_opcode(len=FOLD_ENTRY_N_WORDS, data=entry, out=fold_digest);
    return (fold_digest=fold_digest);
}

// Computes one transaction's leaf output digest, as 8 little-endian u32 words:
// blake2s(encode_felt252s_to_u32s(proof_facts[2:])). The preimage drops the two version
// markers, matching the output digest of the proving side's leaf cairo-verifier circuit.
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

// The circuit hashes below identify the leaf cairo-verifier and multiverifier circuits:
// blake2s(log_blowup_factor || component_log_sizes || preprocessed_root), as eight
// little-endian u32 words. They are circuit identities from the proving side's circuit
// registry; a change to either is a protocol version change.
//
// NOTE: these are the `canonical_small` test-registry values, pinned by tests against
// the Rust mirror (starknet_os::proof_fact_fold); to be replaced with the production
// registry's values before production use.

func get_leaf_verifier_circuit_hash() -> (circuit_hash: felt*) {
    let (circuit_hash) = get_label_location(leaf_verifier_circuit_hash);
    return (circuit_hash=circuit_hash);

    leaf_verifier_circuit_hash:
    dw 0xd2d85a42;
    dw 0x79697b22;
    dw 0x3a41a061;
    dw 0x011cb393;
    dw 0x7a040ec9;
    dw 0x4508f4ca;
    dw 0x42239409;
    dw 0x60f3baea;
}

func get_multiverifier_circuit_hash() -> (circuit_hash: felt*) {
    let (circuit_hash) = get_label_location(multiverifier_circuit_hash);
    return (circuit_hash=circuit_hash);

    multiverifier_circuit_hash:
    dw 0xa5989715;
    dw 0x2377c07a;
    dw 0xc6d1e844;
    dw 0x54f0a04d;
    dw 0x8be65a7d;
    dw 0xfd73c261;
    dw 0x9078e728;
    dw 0x973f680f;
}

// Builds the layer-0 fold entries, one per transaction, consecutively at `leaf_entries`.
func build_leaf_entries{range_check_ptr}(
    n_transactions: felt, proof_facts_references: ProofFactsReference*, leaf_entries: felt*
) {
    alloc_locals;
    if (n_transactions == 0) {
        return ();
    }
    let (leaf_verifier_circuit_hash) = get_leaf_verifier_circuit_hash();
    memcpy(dst=leaf_entries, src=leaf_verifier_circuit_hash, len=BLAKE2S_DIGEST_N_WORDS);
    let (output_digest) = compute_leaf_output_digest(
        proof_facts_size=proof_facts_references.proof_facts_size,
        proof_facts=proof_facts_references.proof_facts,
    );
    memcpy(
        dst=leaf_entries + BLAKE2S_DIGEST_N_WORDS, src=output_digest, len=BLAKE2S_DIGEST_N_WORDS
    );
    return build_leaf_entries(
        n_transactions=n_transactions - 1,
        proof_facts_references=&proof_facts_references[1],
        leaf_entries=leaf_entries + FOLD_ENTRY_N_WORDS,
    );
}

// Unpacks a 128-bit packed digest half into four little-endian u32 words written to
// `digest_words_out`, range-checking that `packed_half` is under 2^128 and every word is
// a u32 (`unsigned_div_rem` range-checks the remainders; the final quotient is checked
// explicitly, which also bounds `packed_half`).
func unpack_digest_half{range_check_ptr}(packed_half: felt, digest_words_out: felt*) {
    let (words_1_to_3_packed, word_0) = unsigned_div_rem(packed_half, 2 ** 32);
    let (words_2_to_3_packed, word_1) = unsigned_div_rem(words_1_to_3_packed, 2 ** 32);
    let (word_3, word_2) = unsigned_div_rem(words_2_to_3_packed, 2 ** 32);
    assert [range_check_ptr] = word_3 + 2 ** 128 - 2 ** 32;
    let range_check_ptr = range_check_ptr + 1;
    assert digest_words_out[0] = word_0;
    assert digest_words_out[1] = word_1;
    assert digest_words_out[2] = word_2;
    assert digest_words_out[3] = word_3;
    return ();
}

// Folds `n_entries` consecutive entries at `entries` into the single root entry. A
// single entry self-folds (the multiverifier verifies the same proof in both slots).
// Precondition: `n_entries` is at least 1.
func fold_entries_to_root{range_check_ptr}(n_entries: felt, entries: felt*) -> (root_entry: felt*) {
    alloc_locals;
    if (n_entries == 1) {
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
// Precondition: `n_entries` is at least 2.
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

// Folds one layer of adjacent pairs left to right into `next_layer_entries`, carrying a
// trailing unpaired entry unchanged; returns the next layer's entry count.
func fold_layer{range_check_ptr}(n_entries: felt, entries: felt*, next_layer_entries: felt*) -> (
    n_next_layer_entries: felt
) {
    if (n_entries == 0) {
        return (n_next_layer_entries=0);
    }
    if (n_entries == 1) {
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

// Folds the two consecutive entries at `children` into `parent_entry`: the
// multiverifier's circuit hash, then blake2s over the children's 32 raw u32 words.
func fold_pair{range_check_ptr}(children: felt*, parent_entry: felt*) {
    let (multiverifier_circuit_hash) = get_multiverifier_circuit_hash();
    memcpy(dst=parent_entry, src=multiverifier_circuit_hash, len=BLAKE2S_DIGEST_N_WORDS);
    blake_with_opcode(
        len=2 * FOLD_ENTRY_N_WORDS, data=children, out=parent_entry + BLAKE2S_DIGEST_N_WORDS
    );
    return ();
}
