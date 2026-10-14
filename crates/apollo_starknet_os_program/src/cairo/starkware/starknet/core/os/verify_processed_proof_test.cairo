from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.cairo_builtins import (
    BitwiseBuiltin,
    EcOpBuiltin,
    HashBuiltin,
    KeccakBuiltin,
    ModBuiltin,
    PoseidonBuiltin,
)
from starkware.starknet.core.os.proof_fact_fold import (
    BLAKE2S_DIGEST_N_WORDS,
    compute_processed_proof_output_digest,
    write_multiverifier_circuit_hash,
)
from starkware.starknet.core.os.verify_processed_proof import verify_processed_proof

// An entry point for testing `verify_processed_proof`. It takes the output builtin without using
// it, since the simple bootloader's hints require the runner to have one.
func verify_processed_proof_test{
    output_ptr: felt*,
    pedersen_ptr: HashBuiltin*,
    range_check_ptr,
    ecdsa_ptr,
    bitwise_ptr: BitwiseBuiltin*,
    ec_op_ptr: EcOpBuiltin*,
    keccak_ptr: KeccakBuiltin*,
    poseidon_ptr: PoseidonBuiltin*,
    range_check96_ptr: felt*,
    add_mod_ptr: ModBuiltin*,
    mul_mod_ptr: ModBuiltin*,
}(proof_facts_size: felt, proof_facts: felt*) {
    alloc_locals;
    // The root entry of the processed proof of the proof facts, which fill both of the
    // multiverifier's verifier slots.
    let (local root_entry: felt*) = alloc();
    write_multiverifier_circuit_hash(circuit_hash=root_entry);
    compute_processed_proof_output_digest(
        left_proof_facts_size=proof_facts_size,
        left_proof_facts=proof_facts,
        right_proof_facts_size=proof_facts_size,
        right_proof_facts=proof_facts,
        output_digest=&root_entry[BLAKE2S_DIGEST_N_WORDS],
    );
    verify_processed_proof(root_entry=root_entry);
    return ();
}
