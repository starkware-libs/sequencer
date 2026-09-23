%builtins output pedersen range_check bitwise poseidon add_mod mul_mod

from starkware.cairo.common.cairo_builtins import (
    BitwiseBuiltin,
    HashBuiltin,
    ModBuiltin,
    PoseidonBuiltin,
)
from program_utils import use_transaction_builtins, write_mod_builtin_instance

// Uses mul_mod once, computing 2 * 3 = 6 (mod 7), and leaves add_mod unused.
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
    write_mod_builtin_instance{mod_ptr=mul_mod_ptr}(result=6);
    assert output_ptr[0] = 6;
    let output_ptr = output_ptr + 1;
    return ();
}
