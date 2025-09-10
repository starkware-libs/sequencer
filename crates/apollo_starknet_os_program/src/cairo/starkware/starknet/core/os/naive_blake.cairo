from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.cairo_blake2s.blake2s import blake_with_opcode

// Encodes a list of felt252s to a list of u32s, each felt is mapped to eight u32s.
// Returns the length of the resulting list of u32s.
func naive_encode_felt252s_to_u32s{range_check_ptr: felt}(
    packed_values_len: felt, packed_values: felt*, unpacked_u32s: felt*
) -> felt {
    alloc_locals;

    local end = cast(packed_values, felt) + packed_values_len;

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

func u256_to_felt(u256: felt*) -> (hash: felt) {
    return (
        hash=u256[7] * 2 ** 224 + u256[6] * 2 ** 192 + u256[5] * 2 ** 160 + u256[4] * 2 ** 128 +
        u256[3] * 2 ** 96 + u256[2] * 2 ** 64 + u256[1] * 2 ** 32 + u256[0],
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
    let (hash: felt) = u256_to_felt(u256=blake_output);
    return (hash=hash);
}
