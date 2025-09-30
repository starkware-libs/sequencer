from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.cairo_blake2s.blake2s import blake_with_opcode

// Computes blake2s of `input` of size 16 felts, representing 32 bits each.
// The initial state is the standard BLAKE2s IV XORed with the parameter block P[0] = 0x01010020.
func blake_with_opcode_for_single_16_length_word(data: felt*, out: felt*, initial_state: felt*) {
    const BLAKE2S_FINALIZE_OPCODE_EXT = 2;
    const OP0_REG = 1;
    const OP1_FP = 3;
    const AP_ADD1 = 11;
    // Flags: 000100000001010
    // - State and data are referenced from fp
    // - Counter is referenced from ap
    // - Increment ap by 1 after instruction
    const BLAKE2S_FLAGS = 2 ** OP0_REG + 2 ** OP1_FP + 2 ** AP_ADD1;

    const STATE_OFFSET = -3;
    const MESSAGE_OFFSET = -5;
    const COUNTER_OFFSET = -1;

    const COUNTER_SHIFT = 1;
    const STATE_SHIFT = 2 ** 16;
    const MESSAGE_SHIFT = 2 ** 32;
    const FLAGS_SHIFT = 2 ** 48;
    const OPCODE_EXT_SHIFT = 2 ** 63;

    const POS_STATE_OFFSET = 2 ** 15 + STATE_OFFSET;
    const POS_MESSAGE_OFFSET = 2 ** 15 + MESSAGE_OFFSET;
    const POS_COUNTER_OFFSET = 2 ** 15 + COUNTER_OFFSET;

    const BLAKE2S_FINALIZE_INSTRUCTION = POS_COUNTER_OFFSET * COUNTER_SHIFT + POS_STATE_OFFSET *
        STATE_SHIFT + POS_MESSAGE_OFFSET * MESSAGE_SHIFT + BLAKE2S_FLAGS * FLAGS_SHIFT +
        BLAKE2S_FINALIZE_OPCODE_EXT * OPCODE_EXT_SHIFT;

    tempvar counter = 64;
    [ap] = out;
    static_assert [ap + COUNTER_OFFSET] == counter;
    static_assert [fp + STATE_OFFSET] == initial_state;
    static_assert [fp + MESSAGE_OFFSET] == data;
    dw BLAKE2S_FINALIZE_INSTRUCTION;
    return ();
}

// / Initializes the BLAKE2s state for a 32-byte (256-bit) digest.
// / This sets the 8-word chaining state `h[0..7]` by XORing the standard BLAKE2s IV
// / with parameter block P[0] = 0x01010020.
// / Returns a pointer to the initialized 8-word state.
func create_initial_state_for_blake2s() -> (initial_state: felt*) {
    // First element eqauls to IV[0] ^ 0x01010020 (config: no key, 32 bytes output).
    tempvar initial_state: felt* = new (
        0x6B08E647,
        0xBB67AE85,
        0x3C6EF372,
        0xA54FF53A,
        0x510E527F,
        0x9B05688C,
        0x1F83D9AB,
        0x5BE0CD19,
    );
    return (initial_state=initial_state);
}

// Encodes a list of felt252s to a list of u32s, each felt is mapped to eight u32s.
// Returns the length of the resulting list of u32s.
func naive_encode_felt252s_to_u32s(
    packed_values_len: felt, packed_values: felt*, unpacked_u32s: felt*
) -> felt {
    alloc_locals;

    local end: felt* = &packed_values[packed_values_len];

    %{ NaiveUnpackFelts252ToU32s %}
    tempvar out = unpacked_u32s;
    tempvar packed_values = packed_values;

    loop:
    if (end == packed_values) {
        return out - unpacked_u32s;
    }

    // TODO(Noa): Assert that the limbs represent a number in the range [0, PRIME-1].
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
