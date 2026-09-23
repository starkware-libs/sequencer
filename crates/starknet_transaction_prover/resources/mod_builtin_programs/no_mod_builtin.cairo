%builtins output pedersen range_check bitwise poseidon add_mod mul_mod

from starkware.cairo.common.bitwise import bitwise_and
from starkware.cairo.common.cairo_builtins import (
    BitwiseBuiltin,
    HashBuiltin,
    ModBuiltin,
    PoseidonBuiltin,
)
from starkware.cairo.common.hash import hash2
from starkware.cairo.common.math import assert_nn
from starkware.cairo.common.builtin_poseidon.poseidon import poseidon_hash

// Uses the pedersen, range_check, bitwise and poseidon builtins so that, together with the privacy
// bootloader, the execution enables exactly the prover components a Starknet transaction enables:
// the small privacy prover only accepts executions that enable that component set.
func use_transaction_builtins{
    pedersen_ptr: HashBuiltin*,
    range_check_ptr,
    bitwise_ptr: BitwiseBuiltin*,
    poseidon_ptr: PoseidonBuiltin*,
}() {
    let (pedersen_result) = hash2{hash_ptr=pedersen_ptr}(1, 2);
    let (poseidon_result) = poseidon_hash(1, 2);
    let (bitwise_result) = bitwise_and(12, 10);
    assert_nn(bitwise_result);
    return ();
}

// Declares the mod builtins without using them.
func main{
    output_ptr: felt*,
    pedersen_ptr: HashBuiltin*,
    range_check_ptr,
    bitwise_ptr: BitwiseBuiltin*,
    poseidon_ptr: PoseidonBuiltin*,
    add_mod_ptr: ModBuiltin*,
    mul_mod_ptr: ModBuiltin*,
}() {
    use_transaction_builtins();
    assert output_ptr[0] = 6;
    let output_ptr = output_ptr + 1;
    return ();
}
