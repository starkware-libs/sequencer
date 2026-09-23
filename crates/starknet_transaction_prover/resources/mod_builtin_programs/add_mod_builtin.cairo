%builtins output pedersen range_check bitwise poseidon add_mod mul_mod

from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.bitwise import bitwise_and
from starkware.cairo.common.cairo_builtins import (
    BitwiseBuiltin,
    HashBuiltin,
    ModBuiltin,
    PoseidonBuiltin,
    UInt384,
)
from starkware.cairo.common.hash import hash2
from starkware.cairo.common.math import assert_nn
from starkware.cairo.common.builtin_poseidon.poseidon import poseidon_hash
from starkware.cairo.common.registers import get_label_location

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

// Writes one add_mod instance directly, computing 2 + 3 = 5 (mod 7), and leaves mul_mod unused.
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

    let (values_ptr: UInt384*) = alloc();
    assert values_ptr[0] = UInt384(d0=2, d1=0, d2=0, d3=0);
    assert values_ptr[1] = UInt384(d0=3, d1=0, d2=0, d3=0);
    assert values_ptr[2] = UInt384(d0=5, d1=0, d2=0, d3=0);

    let (offsets_ptr) = get_label_location(offsets);
    assert add_mod_ptr[0] = ModBuiltin(
        p=UInt384(d0=7, d1=0, d2=0, d3=0), values_ptr=values_ptr, offsets_ptr=offsets_ptr, n=1
    );
    let add_mod_ptr = add_mod_ptr + ModBuiltin.SIZE;

    assert output_ptr[0] = values_ptr[2].d0;
    let output_ptr = output_ptr + 1;
    return ();

    // Felt offsets of the a, b and c operands within values_ptr (a UInt384 spans 4 felts).
    offsets:
    dw 0;
    dw 4;
    dw 8;
}
