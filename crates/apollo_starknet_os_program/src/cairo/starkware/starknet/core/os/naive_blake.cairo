from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.cairo_blake2s.blake2s import blake_with_opcode

// / Encodes a list of felt252s to a list of u32s, each felt is mapped to eight u32s.
func naive_encode_felt252s_to_u32s{range_check_ptr: felt}(
    packed_values_len: felt, packed_values: felt*, unpacked_u32s: felt*
) -> felt {
    alloc_locals;

    local end = cast(packed_values, felt) + packed_values_len;

    // TODO(Einat): add this hint to the enum definition file once function is used in the OS.
    %{ NaiveUnpackFelts252ToU32s %}
    tempvar out = unpacked_u32s;
    tempvar packed_values = packed_values;

    loop:
    if (end - cast(packed_values, felt) == 0) {
        return out - unpacked_u32s;
    }

    // Assert that the limbs represent the number.
    assert packed_values[0] = (
        (out[7] + (2 ** 32 * out[6])) +
        2 ** (32 * 2) * (out[5] + 2 ** 32 * out[4]) +
        2 ** (32 * 4) * (out[3] + 2 ** 32 * out[2]) +
        2 ** (32 * 6) * (out[1] + 2 ** 32 * out[0])
    );

    tempvar out = &out[8];
    tempvar packed_values = &packed_values[1];
    jmp loop;
}

func blake2s_to_felt252(blake_output: felt*) -> (hash: felt) {
    return (
        hash=blake_output[7] * 2 ** 224 + blake_output[6] * 2 ** 192 + blake_output[5] * 2 ** 160 +
        blake_output[4] * 2 ** 128 + blake_output[3] * 2 ** 96 + blake_output[2] * 2 ** 64 +
        blake_output[1] * 2 ** 32 + blake_output[0],
    );
}

// / Encodes a slice of `Felt` values into 32-bit words, then hashes the resulting byte stream
// / with Blake2s-256 and returns the 256-bit digest to a 252-bit field element `Felt`.
func calc_blake_hash{range_check_ptr: felt}(data_len: felt, data: felt*) -> (hash: felt) {
    alloc_locals;
    let (local encoded_data: felt*) = alloc();
    let encoded_data_len = naive_encode_felt252s_to_u32s(
        packed_values_len=data_len, packed_values=data, unpacked_u32s=encoded_data
    );
    let (local blake_output: felt*) = alloc();
    blake_with_opcode(len=encoded_data_len, data=encoded_data, out=blake_output);
    let (hash: felt) = blake2s_to_felt252(blake_output=blake_output);
    return (hash=hash);
}
