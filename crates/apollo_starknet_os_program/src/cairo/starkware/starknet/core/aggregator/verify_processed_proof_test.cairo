from starkware.cairo.common.cairo_builtins import EcOpBuiltin, HashBuiltin, PoseidonBuiltin
from starkware.starknet.core.aggregator.verify_processed_proof import verify_processed_proof

// An entry point for testing `verify_processed_proof`. It takes the output builtin without using
// it, since the simple bootloader's hints require the runner to have one.
func verify_processed_proof_test{
    output_ptr: felt*,
    pedersen_ptr: HashBuiltin*,
    range_check_ptr,
    ec_op_ptr: EcOpBuiltin*,
    poseidon_ptr: PoseidonBuiltin*,
}(processed_proof_output_low: felt, processed_proof_output_high: felt) {
    verify_processed_proof(
        processed_proof_output_low=processed_proof_output_low,
        processed_proof_output_high=processed_proof_output_high,
    );
    return ();
}
