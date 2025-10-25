from starkware.cairo.common.cairo_builtins import EcOpBuiltin
from starkware.cairo.common.ec import ec_mul, recover_y, StarkCurve
from starkware.cairo.common.ec_point import EcPoint
from starkware.cairo.common.math import assert_le_felt, assert_not_zero, assert_le
from starkware.cairo.common.registers import get_fp_and_pc
from starkware.starknet.core.os.naive_blake import (
    calc_blake_hash_single,
    naive_encode_felt252_to_u32s,
    felt_from_le_u32s,
    create_initial_state_for_blake2s,
    blake_with_opcode_for_single_16_length_word,
)
from starkware.cairo.common.cairo_blake2s.blake2s import blake_with_opcode
from starkware.cairo.common.alloc import alloc

// Encryption for StarkNet committee members — Overview
//
// Prerequisites:
//  - Each committee member has a public key (elliptic curve point on StarkCurve)
//
// Part 1: Generate StarkNet keys for each committee member
// - An hint generates a symmetric_key and sn_private_keys by hashing the compressed state diff.
// - The private keys are validated to be in range [1, StarkCurve.ORDER - 1].
// - Public keys are computed from the private keys and output to output_pointer.
//
// Part 2: Share one symmetric_key with multiple committee members
// - For each committee member, derive a shared secret from their public key
//   and the corresponding StarkNet private key.
//   (Shared secret generation uses Elliptic-Curve Diffie–Hellman (ECDH) on StarkCurve).
// - Hash the shared secret's x-coordinate using BLAKE2s to get a mask, then output to output_pointer
//   encrypted_symmetric_key[i] = symmetric_key + mask[i].
//   (A committee member can recompute the same mask with their private key to recover symmetric_key.)
//
// Part 3: Encrypt a list of felts with the symmetric_key
// - For index i, compute mask_i = BLAKE2s(encode([symmetric_key, i])).
// - Output to output_pointer ciphertext[i] = plaintext[i] + mask_i (modulo the field prime).
//
// Output structure:
// encrypted = [n_keys, sn_public_keys, encrypted_symmetric_keys, ciphertext]
//
// Notes:
// - Private keys must be in [1, StarkCurve.ORDER - 1].
// - Public keys are provided by x-coordinate; y is recovered as needed.
// - Field addition is used for masking; this provides confidentiality only.
// - Reference: Elliptic-Curve Diffie–Hellman (ECDH) https://en.wikipedia.org/wiki/Elliptic-curve_Diffie%E2%80%93Hellman
func encrypt_state_diff{range_check_ptr, ec_op_ptr: EcOpBuiltin*}(
    compressed_start: felt*, compressed_end: felt*, n_keys: felt, public_keys: felt*
) -> (encrypted_start: felt*, encrypted_end: felt*) {
    alloc_locals;

    // Generate random symmetric key and random starknet private keys.
    local symmetric_key: felt;
    local sn_private_keys: felt*;
    %{ generate_keys_from_hash(ids.compressed_start, ids.compressed_end, ids.n_keys) %}

    local encrypted_start: felt*;
    %{
        if use_kzg_da:
            ids.encrypted_start = segments.add()
        else:
            # Assign a temporary segment, to be relocated into the output segment.
            ids.encrypted_start = segments.add_temp_segment()
    %}

    let output_pointer = encrypted_start;
    assert output_pointer[0] = n_keys;
    let output_pointer = &output_pointer[1];

    with output_pointer {
        output_sn_public_keys(n_keys=n_keys, sn_private_keys=sn_private_keys);
        output_encrypted_symmetric_key(
            n_keys=n_keys,
            public_keys=public_keys,
            sn_private_keys=sn_private_keys,
            symmetric_key=symmetric_key,
        );
        encrypt(data_start=compressed_start, data_end=compressed_end, symmetric_key=symmetric_key);
    }

    return (encrypted_start=encrypted_start, encrypted_end=output_pointer);
}

// Compute public keys from private keys.
// Step-by-step for each key:
// 1) Multiply the private key by the curve generator to get the public point (x, y).
// 2) Write x into `output` (y can be recovered later when needed).
func output_sn_public_keys{range_check_ptr, ec_op_ptr: EcOpBuiltin*, output_pointer: felt*}(
    n_keys: felt, sn_private_keys: felt*
) {
    if (n_keys == 0) {
        return ();
    }

    let (sn_public_key) = ec_mul(
        m=sn_private_keys[0], p=EcPoint(x=StarkCurve.GEN_X, y=StarkCurve.GEN_Y)
    );
    assert output_pointer[0] = sn_public_key.x;
    let output_pointer = &output_pointer[1];
    // Validates that the private keys are within the range [1, StarkCurve.ORDER - 1].
    assert_not_zero(sn_private_keys[0]);
    assert_le_felt(sn_private_keys[0], StarkCurve.ORDER - 1);
    return output_sn_public_keys(n_keys=n_keys - 1, sn_private_keys=&sn_private_keys[1]);
}

// Encrypt the same symmetric_key for many recipients.
// Step-by-step for each recipient:
// 1) Recover the full public point (x, y) from the given x.
// 2) Compute a shared secret point = our private key * recipient public point (Diffie–Hellman).
// 3) Hash the x-coordinate of the shared point to get a mask.
// 4) encrypted symmetric key = symmetric_key + hash(shared_secret.x)`.
func output_encrypted_symmetric_key{
    range_check_ptr, ec_op_ptr: EcOpBuiltin*, output_pointer: felt*
}(n_keys: felt, public_keys: felt*, sn_private_keys: felt*, symmetric_key: felt) {
    if (n_keys == 0) {
        return ();
    }

    alloc_locals;

    // Using recover_y(x) is safe because on short-Weierstrass curves (x, y) and (x, -y) share the same x.
    // Scalar multiplication depends only on x(Q), so we can reconstruct y later without ambiguity.
    let (public_key) = recover_y(public_keys[0]);

    let (__fp__, _) = get_fp_and_pc();
    let (local shared_secret) = ec_mul(m=sn_private_keys[0], p=public_key);
    let (hash) = calc_blake_hash_single(item=shared_secret.x);

    assert output_pointer[0] = symmetric_key + hash;
    let output_pointer = &output_pointer[1];

    return output_encrypted_symmetric_key(
        n_keys=n_keys - 1,
        public_keys=&public_keys[1],
        sn_private_keys=&sn_private_keys[1],
        symmetric_key=symmetric_key,
    );
}

// Encrypt a list of numbers (felts) using symmetric_key into output_pointer.
// Step-by-step for item i:
// 1) mask = Hash [encoded_symmetric_key, i] .
// 2) ciphertext[i] = plaintext[i] + mask.
func encrypt{range_check_ptr, output_pointer: felt*}(
    data_start: felt*, data_end: felt*, symmetric_key: felt
) {
    // For all elements of the state diff, write the input and output to the same output to
    // optimize segment allocation.
    alloc_locals;
    let (local encoded_symmetric_key: felt*) = alloc();
    let (__fp__, _) = get_fp_and_pc();
    naive_encode_felt252_to_u32s(packed_value=symmetric_key, unpacked_u32s=encoded_symmetric_key);
    let blake_segment: felt* = alloc();
    // Ensure the data size is small - we assume this when encoding the index in encrypt_inner.
    assert_le(data_end - data_start, 2 ** 32 - 1);
    let (initial_state: felt*) = create_initial_state_for_blake2s();
    encrypt_inner(
        data_start=data_start,
        data_end=data_end,
        index=0,
        encoded_symmetric_key=encoded_symmetric_key,
        blake_segment=blake_segment,
        initial_state=initial_state,
    );
    return ();
}

// Helper for `encrypt` that processes one element at a time.
// Stops when we reach `data_end`.
func encrypt_inner{range_check_ptr, output_pointer: felt*}(
    data_start: felt*,
    data_end: felt*,
    index: felt,
    encoded_symmetric_key: felt*,
    blake_segment: felt*,
    initial_state: felt*,
) {
    if (data_start == data_end) {
        return ();
    }
    let blake_input = blake_segment;

    // Write encoded symmetric key to blake output.
    assert blake_segment[0] = encoded_symmetric_key[0];
    assert blake_segment[1] = encoded_symmetric_key[1];
    assert blake_segment[2] = encoded_symmetric_key[2];
    assert blake_segment[3] = encoded_symmetric_key[3];
    assert blake_segment[4] = encoded_symmetric_key[4];
    assert blake_segment[5] = encoded_symmetric_key[5];
    assert blake_segment[6] = encoded_symmetric_key[6];
    assert blake_segment[7] = encoded_symmetric_key[7];
    let blake_segment = &blake_segment[8];
    // Write encoded index to blake output - since index is small, manually encode as [0, 0, 0, 0, 0, 0, 0, index].
    assert blake_segment[0] = index;
    assert blake_segment[1] = 0;
    assert blake_segment[2] = 0;
    assert blake_segment[3] = 0;
    assert blake_segment[4] = 0;
    assert blake_segment[5] = 0;
    assert blake_segment[6] = 0;
    assert blake_segment[7] = 0;
    let blake_segment = &blake_segment[8];
    // Calculate blake hash modulo prime.
    blake_with_opcode_for_single_16_length_word(
        data=blake_input, out=blake_segment, initial_state=initial_state
    );
    // This line will result in c_i = blake(k,i) % PRIME + p_i.
    // Meaning we are mapping the u256 to PRIME, as PRIME doesn't fit completely in u256
    // there will be a slight section that has lower probability, but this is negligible for 2 reasons:
    // 1. The difference is very small compared to the size of PRIME, as PRIME is close to a power of 2,
    //    meaning the probability to fall in this section is very low.
    // 2. The numbers of 'rows' in this transformation is relatively high so the difference
    //    between the probabilities is relatively small.
    // Note: if assumption 1 changes we will need to re-evaluate this.
    let hash = felt_from_le_u32s(u32s=blake_segment);
    let blake_segment = &blake_segment[8];

    // Encrypt the current element.
    assert output_pointer[0] = hash + data_start[0];

    let output_pointer = &output_pointer[1];

    return encrypt_inner(
        data_start=&data_start[1],
        data_end=data_end,
        index=index + 1,
        encoded_symmetric_key=encoded_symmetric_key,
        blake_segment=blake_segment,
        initial_state=initial_state,
    );
}
