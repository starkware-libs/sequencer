from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.cairo_blake2s.blake2s import blake_with_opcode, encode_felt252s_to_u32s
from starkware.cairo.common.math import assert_nn_le
from starkware.cairo.common.memcpy import memcpy
from starkware.cairo.common.registers import get_label_location

const BLAKE2S_DIGEST_N_WORDS = 8;
const FOLD_ENTRY_N_WORDS = 2 * BLAKE2S_DIGEST_N_WORDS;
const N_LEAF_VERIFIER_CIRCUITS = 1;

func single_transaction_root_entry{range_check_ptr}(
    proof_facts_size: felt, proof_facts: felt*, leaf_circuit_index: felt
) -> (root_entry: felt*) {
    alloc_locals;
    let (local leaf_entry: felt*) = alloc();
    let (leaf_verifier_circuit_hash) = get_leaf_verifier_circuit_hash(
        leaf_circuit_index=leaf_circuit_index
    );
    memcpy(dst=leaf_entry, src=leaf_verifier_circuit_hash, len=BLAKE2S_DIGEST_N_WORDS);
    let (output_digest) = compute_leaf_output_digest(
        proof_facts_size=proof_facts_size, proof_facts=proof_facts
    );
    memcpy(dst=leaf_entry + BLAKE2S_DIGEST_N_WORDS, src=output_digest, len=BLAKE2S_DIGEST_N_WORDS);

    let (local self_fold_preimage: felt*) = alloc();
    memcpy(dst=self_fold_preimage, src=leaf_entry, len=FOLD_ENTRY_N_WORDS);
    memcpy(dst=self_fold_preimage + FOLD_ENTRY_N_WORDS, src=leaf_entry, len=FOLD_ENTRY_N_WORDS);
    let (local root_entry: felt*) = alloc();
    let (multiverifier_circuit_hash) = get_multiverifier_circuit_hash();
    memcpy(dst=root_entry, src=multiverifier_circuit_hash, len=BLAKE2S_DIGEST_N_WORDS);
    blake_with_opcode(
        len=2 * FOLD_ENTRY_N_WORDS, data=self_fold_preimage, out=root_entry + BLAKE2S_DIGEST_N_WORDS
    );
    return (root_entry=root_entry);
}

func compute_fold_digest{range_check_ptr}(entry: felt*) -> (fold_digest: felt*) {
    alloc_locals;
    let (local fold_digest: felt*) = alloc();
    blake_with_opcode(len=FOLD_ENTRY_N_WORDS, data=entry, out=fold_digest);
    return (fold_digest=fold_digest);
}

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

func get_leaf_verifier_circuit_hash{range_check_ptr}(leaf_circuit_index: felt) -> (
    circuit_hash: felt*
) {
    assert_nn_le(leaf_circuit_index, N_LEAF_VERIFIER_CIRCUITS - 1);
    let (leaf_verifier_circuit_hashes) = get_label_location(leaf_verifier_circuit_hashes_table);
    return (
        circuit_hash=leaf_verifier_circuit_hashes + leaf_circuit_index * BLAKE2S_DIGEST_N_WORDS
    );

    leaf_verifier_circuit_hashes_table:
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
