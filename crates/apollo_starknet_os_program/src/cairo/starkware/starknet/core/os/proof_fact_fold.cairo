// The OS mirror of the recursive proof tree the proving side builds over privacy
// transactions' proof facts (`stwo_run_and_prove_recursive_tree`): reproduces the tree's
// digests bit for bit, so they can be compared with the circuit verifier's public output.

from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.cairo_blake2s.blake2s import blake_with_opcode, encode_felt252s_to_u32s
from starkware.cairo.common.math import assert_nn_le
from starkware.cairo.common.registers import get_label_location

const BLAKE2S_DIGEST_N_WORDS = 8;

// The number of leaf cairo-verifier circuits in the allowed-circuits table below. A leaf
// circuit is chosen per proof by its trace size, so the table holds every allowed one.
const N_LEAF_VERIFIER_CIRCUITS = 1;

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
// registry; a change to any of them is a protocol version change.
//
// NOTE: these are the `canonical_small` test-registry values, pinned by tests against
// the Rust mirror (starknet_os::proof_fact_fold); to be replaced with the production
// registry's values before production use.

// Returns the hash of the leaf cairo-verifier circuit at `leaf_circuit_index` in the
// allowed-circuits table (the registry's leaf verifiers, in registry order).
// `leaf_circuit_index` is unconstrained input (a hint), so it is range-checked to be
// within the table.
func get_leaf_verifier_circuit_hash{range_check_ptr}(leaf_circuit_index: felt) -> (
    circuit_hash: felt*
) {
    assert_nn_le(leaf_circuit_index, N_LEAF_VERIFIER_CIRCUITS - 1);
    let (leaf_verifier_circuit_hashes) = get_label_location(leaf_verifier_circuit_hashes_table);
    return (
        circuit_hash=leaf_verifier_circuit_hashes + leaf_circuit_index * BLAKE2S_DIGEST_N_WORDS
    );

    leaf_verifier_circuit_hashes_table:
    // Index 0.
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
