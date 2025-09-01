func naive_encode_felt252_to_u32s{range_check_ptr}(packed_value: felt, unpacked_u32s: felt*) -> (
    num_u32s: felt
) {
    // TODO(Einat): add this hint to the enum definition file once function is used in the OS.
    %{ naive_unpack_felt252_to_u32s(ids.packed_value, ids.unpacked_u32s) %}
    tempvar out = unpacked_u32s;
    assert packed_value = out[7] + 2 ** 32 * out[6] + 2 ** (32 * 2) * out[5] + 2 ** (32 * 3) * out[
        4
    ] + 2 ** (32 * 4) * out[3] + 2 ** (32 * 5) * out[2] + 2 ** (32 * 6) * out[1] + 2 ** (32 * 7) *
        out[0];
    return (num_u32s=8);
}
