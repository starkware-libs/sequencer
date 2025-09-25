from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.cairo_blake2s.blake2s import blake_with_opcode
from starkware.cairo.common.cairo_blake2s.blake2s import BLAKE2S_FINALIZE_INSTRUCTION

// Computes blake2s of `input` of size 16 felts, representing 32 bits each.
// The initial state is the standard BLAKE2s IV XORed with the parameter block P[0] = 0x01010020.
func blake_with_opcode_for_single_16_length_word(data: felt*, out: felt*, initial_state: felt*) {
    tempvar counter = 64;
    [ap] = initial_state, ap++;
    [ap] = data, ap++;
    [ap] = counter, ap++;
    [ap] = out;
    dw BLAKE2S_FINALIZE_INSTRUCTION;
    // Increment AP after blake opcode.
    ap += 1;
    return ();
}

// / Initializes the BLAKE2s state for a 32-byte (256-bit) digest.
// / This sets the 8-word chaining state `h[0..7]` by XORing the standard BLAKE2s IV
// / with parameter block P[0] = 0x01010020.
// / Returns a pointer to the initialized 8-word state.
func create_initial_state_for_blake2s() -> (initial_state: felt*) {
    alloc_locals;
    let (local initial_state: felt*) = alloc();
    assert initial_state[0] = 0x6B08E647;  // IV[0] ^ 0x01010020 (config: no key, 32 bytes output).
    assert initial_state[1] = 0xBB67AE85;
    assert initial_state[2] = 0x3C6EF372;
    assert initial_state[3] = 0xA54FF53A;
    assert initial_state[4] = 0x510E527F;
    assert initial_state[5] = 0x9B05688C;
    assert initial_state[6] = 0x1F83D9AB;
    assert initial_state[7] = 0x5BE0CD19;
    return (initial_state=initial_state);
}

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

// Gets a felt that represent a 256-bit unsigned integer stored as an array of eight 32-bit unsigned integers
// represented in little-endian notation. Return the felt representation of the integer modulo prime.
func u256_to_felt(u256: felt*) -> felt {
    let hash = u256[7] * 2 ** 224 + u256[6] * 2 ** 192 + u256[5] * 2 ** 160 + u256[4] * 2 ** 128 +
        u256[3] * 2 ** 96 + u256[2] * 2 ** 64 + u256[1] * 2 ** 32 + u256[0];
    return hash;
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
    let hash = u256_to_felt(u256=blake_output);
    return (hash=hash);
}
