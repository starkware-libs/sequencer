from starkware.cairo.common.cairo_blake2s.blake2s import encode_felt252_data_and_calc_blake_hash
from starkware.cairo.common.cairo_builtins import EcOpBuiltin
from starkware.cairo.common.ec import ec_mul, recover_y, StarkCurve
from starkware.cairo.common.ec_point import EcPoint
from starkware.cairo.common.math import assert_le_felt, assert_not_zero
from starkware.cairo.common.registers import get_fp_and_pc

// Validates that the private keys are within the range [1, StarkCurve.ORDER - 1] as required by
// the Diffie-Hellman elliptic curve encryption scheme.
func validate_private_keys{range_check_ptr}(n_keys: felt, sn_private_keys: felt*) {
    if (n_keys == 0) {
        return ();
    }
    assert_not_zero(sn_private_keys[0]);
    assert_le_felt(sn_private_keys[0], StarkCurve.ORDER - 1);

    return validate_private_keys(n_keys=n_keys - 1, sn_private_keys=sn_private_keys + 1);
}

// Computes the public keys from the private keys by multiplying by the EC group generator.
func compute_public_keys{range_check_ptr, ec_op_ptr: EcOpBuiltin*, encrypted_dst: felt*}(
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
    return compute_public_keys(n_keys=n_keys - 1, sn_private_keys=&sn_private_keys[1]);
}

func encrypt_symmetric_key{range_check_ptr, ec_op_ptr: EcOpBuiltin*, encrypted_dst: felt*}(
    n_keys: felt, public_keys: felt*, sn_private_keys: felt*, symmetric_key: felt
) {
    if (n_keys == 0) {
        return ();
    }

    alloc_locals;

    let (public_key) = recover_y(public_keys[0]);

    let (__fp__, _) = get_fp_and_pc();
    let (local shared_secret) = ec_mul(m=sn_private_keys[0], p=public_key);
    // TODO(Avi, 10/9/2025): Switch to naive encoding once the function is available.
    let (hash) = encode_felt252_data_and_calc_blake_hash(data_len=1, data=&shared_secret.x);

    assert encrypted_dst[0] = symmetric_key + hash;
    let encrypted_dst = &encrypted_dst[1];

    return encrypt_symmetric_key(
        n_keys=n_keys - 1,
        public_keys=&public_keys[1],
        sn_private_keys=&sn_private_keys[1],
        symmetric_key=symmetric_key,
    );
}

func encrypt{range_check_ptr, encrypted_dst: felt*}(
    data_start: felt*, data_end: felt*, symmetric_key: felt
) {
    encrypt_inner(data_start=data_start, data_end=data_end, index=0, symmetric_key=symmetric_key);
    return ();
}

// A helper for encrypt.
func encrypt_inner{range_check_ptr, encrypted_dst: felt*}(
    data_start: felt*, data_end: felt*, index: felt, symmetric_key: felt
) {
    if (data_start == data_end) {
        return ();
    }

    // TODO(Noa): prepare the entire input in a single array
    tempvar blake_input: felt* = new (symmetric_key, index);
    // Encrypt the current element.
    let (hash: felt) = encode_felt252_data_and_calc_blake_hash(data_len=2, data=blake_input);
    assert encrypted_dst[0] = hash + data_start[0];

    let encrypted_dst = &encrypted_dst[1];

    return encrypt_inner(
        data_start=&data_start[1], data_end=data_end, index=index + 1, symmetric_key=symmetric_key
    );
}
