%builtins output pedersen range_check ecdsa bitwise ec_op keccak poseidon range_check96 add_mod mul_mod

from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.bitwise import bitwise_and
from starkware.cairo.common.builtin_poseidon.poseidon import poseidon_hash
from starkware.cairo.common.cairo_builtins import (
    BitwiseBuiltin,
    EcOpBuiltin,
    HashBuiltin,
    KeccakBuiltin,
    ModBuiltin,
    PoseidonBuiltin,
    SignatureBuiltin,
    UInt384,
)
from starkware.cairo.common.ec_point import EcPoint
from starkware.cairo.common.hash import hash2
from starkware.cairo.common.keccak_state import KeccakBuiltinState
from starkware.cairo.common.math import assert_nn
from starkware.cairo.common.registers import get_label_location
from starkware.cairo.common.signature import verify_ecdsa_signature

// Values of the `selected_builtin` program input. `src/proving/prover_test.rs` passes the same
// values.
const NO_BUILTIN = 0;
const ECDSA = 1;
const RANGE_CHECK96 = 2;
const ADD_MOD = 3;
const MUL_MOD = 4;

// Uses every builtin that the small privacy prover supports, and one instance of the builtin that
// the `selected_builtin` program input selects.
func main{
    output_ptr: felt*,
    pedersen_ptr: HashBuiltin*,
    range_check_ptr,
    ecdsa_ptr: SignatureBuiltin*,
    bitwise_ptr: BitwiseBuiltin*,
    ec_op_ptr: EcOpBuiltin*,
    keccak_ptr: KeccakBuiltin*,
    poseidon_ptr: PoseidonBuiltin*,
    range_check96_ptr: felt*,
    add_mod_ptr: ModBuiltin*,
    mul_mod_ptr: ModBuiltin*,
}() {
    alloc_locals;
    local selected_builtin;
    %{ ids.selected_builtin = program_input["selected_builtin"] %}

    use_supported_builtins();
    use_selected_builtin(selected_builtin);
    assert output_ptr[0] = selected_builtin;
    let output_ptr = output_ptr + 1;
    return ();
}

// Uses the pedersen, range_check, bitwise, ec_op, keccak and poseidon builtins, which the small
// privacy prover supports. Together with the privacy bootloader, the execution enables exactly the
// small prover's component set, which is the only set that the small prover accepts.
func use_supported_builtins{
    pedersen_ptr: HashBuiltin*,
    range_check_ptr,
    bitwise_ptr: BitwiseBuiltin*,
    ec_op_ptr: EcOpBuiltin*,
    keccak_ptr: KeccakBuiltin*,
    poseidon_ptr: PoseidonBuiltin*,
}() {
    alloc_locals;
    let (pedersen_result) = hash2{hash_ptr=pedersen_ptr}(1, 2);
    let (poseidon_result) = poseidon_hash(1, 2);
    let (bitwise_result) = bitwise_and(12, 10);
    assert_nn(bitwise_result);
    write_ec_op_instance();
    write_keccak_instance();
    return ();
}

// Uses `selected_builtin` once, or nothing for NO_BUILTIN.
func use_selected_builtin{
    ecdsa_ptr: SignatureBuiltin*,
    range_check96_ptr: felt*,
    add_mod_ptr: ModBuiltin*,
    mul_mod_ptr: ModBuiltin*,
}(selected_builtin: felt) {
    if (selected_builtin == ECDSA) {
        // A signature on message 5 by the private key 0x1234567890abcdef.
        verify_ecdsa_signature{ecdsa_ptr=ecdsa_ptr}(
            message=5,
            public_key=0x2abbefdcbf731195ee2acd186441eb536e86f888327b3655cffbd07b57dbf26,
            signature_r=0xbce308299c5825a44a405c6f7c3dc2afe3561eb60cb24e3399f6a5f4ba25e8,
            signature_s=0x5eeea0e51ce6a19c59acb66f2147518e0cea9caef5c39c1f74ac76746db5d55,
        );
        return ();
    }
    if (selected_builtin == RANGE_CHECK96) {
        assert range_check96_ptr[0] = 2 ** 96 - 1;
        let range_check96_ptr = range_check96_ptr + 1;
        return ();
    }
    if (selected_builtin == ADD_MOD) {
        write_mod_builtin_instance{mod_ptr=add_mod_ptr}(result=5);
        return ();
    }
    if (selected_builtin == MUL_MOD) {
        write_mod_builtin_instance{mod_ptr=mul_mod_ptr}(result=6);
        return ();
    }
    assert selected_builtin = NO_BUILTIN;
    return ();
}

// Writes one ec_op instance directly, computing G + 1 * 2G = 3G, where G is the STARK curve
// generator. The builtin checks that `r` is the outcome of that operation.
func write_ec_op_instance{ec_op_ptr: EcOpBuiltin*}() {
    assert ec_op_ptr[0] = EcOpBuiltin(
        p=EcPoint(
            x=0x1ef15c18599971b7beced415a40f0c7deacfd9b0d1819e03d723d8bc943cfca,
            y=0x5668060aa49730b7be4801df46ec62de53ecd11abe43a32873000c36e8dc1f,
        ),
        q=EcPoint(
            x=0x759ca09377679ecd535a81e83039658bf40959283187c654c5416f439403cf5,
            y=0x6f524a3400e7708d5c01a28598ad272e7455aa88778b19f93b562d7a9646c41,
        ),
        m=1,
        r=EcPoint(
            x=0x411494b501a98abd8262b0da1351e17899a0c4ef23dd2f96fec5ba847310b20,
            y=0x7e1b3ebac08924d2c26f409549191fcf94f3bf6f301ed3553e22dfb802f0686,
        ),
    );
    let ec_op_ptr = ec_op_ptr + EcOpBuiltin.SIZE;
    return ();
}

// Writes one keccak instance directly, applying the keccak permutation to the zero state. Reading
// each output cell makes the builtin deduce it, so that the instance has no unset cells.
func write_keccak_instance{keccak_ptr: KeccakBuiltin*}() {
    assert keccak_ptr[0].input = KeccakBuiltinState(0, 0, 0, 0, 0, 0, 0, 0);
    tempvar output_s0 = keccak_ptr[0].output.s0;
    tempvar output_s1 = keccak_ptr[0].output.s1;
    tempvar output_s2 = keccak_ptr[0].output.s2;
    tempvar output_s3 = keccak_ptr[0].output.s3;
    tempvar output_s4 = keccak_ptr[0].output.s4;
    tempvar output_s5 = keccak_ptr[0].output.s5;
    tempvar output_s6 = keccak_ptr[0].output.s6;
    tempvar output_s7 = keccak_ptr[0].output.s7;
    let keccak_ptr = keccak_ptr + KeccakBuiltin.SIZE;
    return ();
}

// Writes one mod builtin instance directly to mod_ptr, applying the builtin's operation to 2 and 3
// modulo 7. The builtin checks that `result` is the outcome of that operation. Going through
// `core::circuit` instead would also use mul_mod for an add gate, to reduce its inputs.
func write_mod_builtin_instance{mod_ptr: ModBuiltin*}(result: felt) {
    let (values_ptr: UInt384*) = alloc();
    assert values_ptr[0] = UInt384(d0=2, d1=0, d2=0, d3=0);
    assert values_ptr[1] = UInt384(d0=3, d1=0, d2=0, d3=0);
    assert values_ptr[2] = UInt384(d0=result, d1=0, d2=0, d3=0);

    let (offsets_ptr) = get_label_location(offsets);
    assert mod_ptr[0] = ModBuiltin(
        p=UInt384(d0=7, d1=0, d2=0, d3=0), values_ptr=values_ptr, offsets_ptr=offsets_ptr, n=1
    );
    let mod_ptr = mod_ptr + ModBuiltin.SIZE;
    return ();

    // Felt offsets of the a, b and c operands within values_ptr (a UInt384 spans 4 felts).
    offsets:
    dw 0;
    dw 4;
    dw 8;
}
