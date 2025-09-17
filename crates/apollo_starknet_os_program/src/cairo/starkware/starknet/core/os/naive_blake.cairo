// / Encodes a list of felt252s to a list of u32s, each felt is mapped to eight u32s.
func naive_encode_felt252s_to_u32s(
    packed_values_len: felt, packed_values: felt*, unpacked_u32s: felt*
) -> felt {
    alloc_locals;

    local end = cast(packed_values, felt) + packed_values_len;

    // TODO(Einat): add this hint to the enum definition file once function is used in the OS.
    %{ naive_encode_felt252s_to_u32s(ids.packed_values_len, ids.packed_values, ids.unpacked_u32s) %}
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
