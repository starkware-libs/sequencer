// Encodes a list of felt252s to a list of u32s without special handling for small felt252s.
func naive_encode_felt252s_to_u32s{range_check_ptr: felt}(
    packed_values_len: felt, packed_values: felt*, unpacked_u32s: felt*
) -> felt {
    alloc_locals;

    local EXP31 = 2 ** 31;
    local end = cast(packed_values, felt) + packed_values_len;

    // TODO(Einat): add this hint to the enum definition file once function is used in the OS.
    %{ naive_encode_felt252s_to_u32s(ids.packed_values_len, ids.packed_values, ids.unpacked_u32s) %}
    tempvar out = unpacked_u32s;
    tempvar packed_values = packed_values;
    tempvar range_check_ptr = range_check_ptr;

    loop:
    if (end - cast(packed_values, felt) == 0) {
        return out - unpacked_u32s;
    }

    // Assert that the top limb is over 2^31, as its MSB is artificially set for encoding.
    tempvar raw_out_0 = out[0] - EXP31;
    assert [range_check_ptr] = raw_out_0;
    // Assert that the limbs represent the number. Set the MSB of the most significant limb.
    assert packed_values[0] = (
        (out[7] + (2 ** 32 * out[6])) +
        2 ** (32 * 2) * (out[5] + 2 ** 32 * out[4]) +
        2 ** (32 * 4) * (out[3] + 2 ** 32 * out[2]) +
        2 ** (32 * 6) * (out[1] + 2 ** 32 * raw_out_0)
    );

    tempvar out = &out[8];
    tempvar packed_values = &packed_values[1];
    tempvar range_check_ptr = range_check_ptr + 1;
    jmp loop;
}
