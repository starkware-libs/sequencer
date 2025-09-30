from starkware.cairo.common.cairo_builtins import EcOpBuiltin
from starkware.cairo.common.ec import ec_mul, recover_y, StarkCurve
from starkware.cairo.common.ec_point import EcPoint
from starkware.cairo.common.math import assert_le_felt, assert_not_zero, assert_le
from starkware.cairo.common.registers import get_fp_and_pc
from starkware.starknet.core.os.naive_blake import (
    calc_blake_hash,
    naive_encode_felt252s_to_u32s,
    u256_to_felt,
    create_initial_state_for_blake2s,
    blake_with_opcode_for_single_16_length_word,
)
from starkware.cairo.common.cairo_blake2s.blake2s import blake_with_opcode
from starkware.cairo.common.alloc import alloc

// Encrypts state diff data by generating keys, outputting public keys,
// encrypting symmetric keys, and encrypting the data.
func encrypt_state_diff{range_check_ptr, ec_op_ptr: EcOpBuiltin*}(
    compressed_start: felt*, compressed_dst: felt*, n_keys: felt, public_keys: felt*
) -> (encrypted_start: felt*, encrypted_dst: felt*) {
    alloc_locals;

    // Generate random symmetric key and random starknet private keys.
    local symmetric_key: felt;
    local sn_private_keys: felt*;
    %{ generate_keys_from_hash(ids.compressed_start, ids.compressed_dst, ids.n_keys) %}
    validate_sn_private_keys(n_keys=n_keys, sn_private_keys=sn_private_keys);

    local encrypted_start: felt*;
    %{
        if use_kzg_da:
            ids.encrypted_start = segments.add()
        else:
            # Assign a temporary segment, to be relocated into the output segment.
            ids.encrypted_start = segments.add_temp_segment()
    %}

    let encrypted_dst = encrypted_start;
    assert encrypted_dst[0] = n_keys;
    let encrypted_dst = &encrypted_dst[1];

    with encrypted_dst {
        output_sn_public_keys(n_keys=n_keys, sn_private_keys=sn_private_keys);
        output_encrypted_symmetric_key(
            n_keys=n_keys,
            public_keys=public_keys,
            sn_private_keys=sn_private_keys,
            symmetric_key=symmetric_key,
        );
        encrypt(data_start=compressed_start, data_end=compressed_dst, symmetric_key=symmetric_key);
    }

    return (encrypted_start=encrypted_start, encrypted_dst=encrypted_dst);
}

// Validates that the private keys are within the range [1, StarkCurve.ORDER - 1] as required by
// the Diffie-Hellman elliptic curve encryption scheme.
func validate_sn_private_keys{range_check_ptr}(n_keys: felt, sn_private_keys: felt*) {
    if (n_keys == 0) {
        return ();
    }
    assert_not_zero(sn_private_keys[0]);
    assert_le_felt(sn_private_keys[0], StarkCurve.ORDER - 1);

    return validate_sn_private_keys(n_keys=n_keys - 1, sn_private_keys=&sn_private_keys[1]);
}

// Computes the public keys from the private keys by multiplying by the EC group generator.
func output_sn_public_keys{range_check_ptr, ec_op_ptr: EcOpBuiltin*, encrypted_dst: felt*}(
    n_keys: felt, sn_private_keys: felt*
) {
    if (n_keys == 0) {
        return ();
    }
    let (sn_public_key) = ec_mul(
        m=sn_private_keys[0], p=EcPoint(x=StarkCurve.GEN_X, y=StarkCurve.GEN_Y)
    );
    assert encrypted_dst[0] = sn_public_key.x;
    let encrypted_dst = &encrypted_dst[1];
    return output_sn_public_keys(n_keys=n_keys - 1, sn_private_keys=&sn_private_keys[1]);
}

func output_encrypted_symmetric_key{range_check_ptr, ec_op_ptr: EcOpBuiltin*, encrypted_dst: felt*}(
    n_keys: felt, public_keys: felt*, sn_private_keys: felt*, symmetric_key: felt
) {
    if (n_keys == 0) {
        return ();
    }

    alloc_locals;

    let (public_key) = recover_y(public_keys[0]);

    let (__fp__, _) = get_fp_and_pc();
    let (local shared_secret) = ec_mul(m=sn_private_keys[0], p=public_key);
    let (hash) = calc_blake_hash(data_len=1, data=&shared_secret.x);

    assert encrypted_dst[0] = symmetric_key + hash;
    let encrypted_dst = &encrypted_dst[1];

    return output_encrypted_symmetric_key(
        n_keys=n_keys - 1,
        public_keys=&public_keys[1],
        sn_private_keys=&sn_private_keys[1],
        symmetric_key=symmetric_key,
    );
}

func encrypt{range_check_ptr, encrypted_dst: felt*}(
    data_start: felt*, data_end: felt*, symmetric_key: felt
) {
    // For all elements of the state diff, write the input and output to the same output to
    // optimize segment allocation.
    alloc_locals;
    let (local encoded_symmetric_key: felt*) = alloc();
    let (__fp__, _) = get_fp_and_pc();
    naive_encode_felt252s_to_u32s(
        packed_values_len=1, packed_values=&symmetric_key, unpacked_u32s=encoded_symmetric_key
    );
    let blake_output: felt* = alloc();
    // Ensure the data size is small - we assume this when encoding the index in encrypt_inner.
    assert_le(data_end - data_start, 2 ** 32 - 1);
    let (initial_state: felt*) = create_initial_state_for_blake2s();
    encrypt_inner(
        data_start=data_start,
        data_end=data_end,
        index=0,
        encoded_symmetric_key=encoded_symmetric_key,
        blake_output=blake_output,
        initial_state=initial_state,
    );
    return ();
}

// A helper for encrypt.
func encrypt_inner{range_check_ptr, encrypted_dst: felt*}(
    data_start: felt*,
    data_end: felt*,
    index: felt,
    encoded_symmetric_key: felt*,
    blake_output: felt*,
    initial_state: felt*,
) {
    if (data_start == data_end) {
        return ();
    }
    let blake_encoding_start = blake_output;

    // Write encoded symmetric key to blake output.
    assert blake_output[0] = encoded_symmetric_key[0];
    assert blake_output[1] = encoded_symmetric_key[1];
    assert blake_output[2] = encoded_symmetric_key[2];
    assert blake_output[3] = encoded_symmetric_key[3];
    assert blake_output[4] = encoded_symmetric_key[4];
    assert blake_output[5] = encoded_symmetric_key[5];
    assert blake_output[6] = encoded_symmetric_key[6];
    assert blake_output[7] = encoded_symmetric_key[7];
    let blake_output = &blake_output[8];
    // Write encoded index to blake output - since index is small, manually encode as [0, 0, 0, 0, 0, 0, 0, index].
    assert blake_output[0] = 0;
    assert blake_output[1] = 0;
    assert blake_output[2] = 0;
    assert blake_output[3] = 0;
    assert blake_output[4] = 0;
    assert blake_output[5] = 0;
    assert blake_output[6] = 0;
    assert blake_output[7] = index;
    let blake_output = &blake_output[8];
    // Calculate blake hash modulo prime.
    blake_with_opcode_for_single_16_length_word(
        data=blake_encoding_start, out=blake_output, initial_state=initial_state
    );
    let hash = u256_to_felt(u256=blake_output);
    let blake_output = &blake_output[8];

    // Encrypt the current element.
    assert encrypted_dst[0] = hash + data_start[0];

    let encrypted_dst = &encrypted_dst[1];

    return encrypt_inner(
        data_start=&data_start[1],
        data_end=data_end,
        index=index + 1,
        encoded_symmetric_key=encoded_symmetric_key,
        blake_output=blake_output,
        initial_state=initial_state,
    );
}
