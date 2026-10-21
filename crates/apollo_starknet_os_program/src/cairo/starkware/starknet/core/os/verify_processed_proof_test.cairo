from starkware.cairo.common.cairo_builtins import (
    BitwiseBuiltin,
    EcOpBuiltin,
    HashBuiltin,
    KeccakBuiltin,
    ModBuiltin,
    PoseidonBuiltin,
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
    verify_processed_proof(proof_facts_size=proof_facts_size, proof_facts=proof_facts);
    return ();
}
