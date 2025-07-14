use blake2::Blake2s256;
use digest::Digest;
use starknet_types_core::felt::Felt;

// Encode each `Felt` into 32-bit words:
/// - **Small** values `< 2^63` get **2** words: `[ high_32_bits, low_32_bits ]` from the last 8
///   bytes of the 256-bit BE representation.
/// - **Large** values `>= 2^63` get **8** words: the full 32-byte big-endian split, **with** the
///   MSB of the first word set as a marker (`+2^255`).
///
/// # Returns
/// A flat `Vec<u32>` containing all the unpacked words, in the same order.
pub fn encode_felts_to_u32s(felts: Vec<Felt>) -> Vec<u32> {
    // 2**63.
    const SMALL_THRESHOLD: Felt = Felt::from_hex_unchecked("8000000000000000");
    // MSB mask for the first u32 in the 8-limb case.
    const BIG_MARKER: u32 = 1 << 31;

    let mut unpacked_u32s = Vec::new();
    for felt in felts {
        let felt_as_be_bytes = felt.to_bytes_be();
        if felt < SMALL_THRESHOLD {
            // small: 2 limbs only, high‐32 then low‐32 of the last 8 bytes.
            let hi = u32::from_be_bytes(felt_as_be_bytes[24..28].try_into().unwrap());
            let lo = u32::from_be_bytes(felt_as_be_bytes[28..32].try_into().unwrap());
            unpacked_u32s.push(hi);
            unpacked_u32s.push(lo);
        } else {
            // big: 8 limbs, big‐endian order.
            let start = unpacked_u32s.len();
            for chunk in felt_as_be_bytes.chunks_exact(4) {
                unpacked_u32s.push(u32::from_be_bytes(chunk.try_into().unwrap()));
            }
            // set the MSB of the very first limb as the Cairo hint does with "+ 2**255".
            unpacked_u32s[start] |= BIG_MARKER;
        }
    }
    unpacked_u32s
}

/// Packs the first 8 little-endian 32-bit words (32 bytes) of `bytes`
/// into a single Felt (252 bits).
fn pack_256_le_to_felt(bytes: &[u8]) -> Felt {
    assert!(bytes.len() >= 32, "need at least 32 bytes to pack 8 words");
    // 1) copy your 32-byte LE-hash into the low 32 bytes of a 32-byte buffer
    let mut buf = [0u8; 32];
    buf[..32].copy_from_slice(&bytes[..32]);
    // 2) interpret the whole 32-byte buffer as a little-endian Felt
    Felt::from_bytes_le(&buf)
}

pub fn blake2s_to_felt(data: &[u8]) -> Felt {
    let mut hasher = Blake2s256::new();
    hasher.update(data);
    let hash32 = hasher.finalize();
    pack_256_le_to_felt(hash32.as_slice())
}

/// Encodes a slice of `Felt` values into 32-bit words exactly as Cairo’s
/// `encode_felt252_to_u32s` hint does, then hashes the resulting byte stream
/// with Blake2s-256 and returns the 256-bit truncated digest as a `Felt`.
pub fn encode_felt252_data_and_calc_blake_hash(data: &[Felt]) -> Felt {
    // 1) Unpack each Felt into 2 or 8 u32 limbs
    let u32_words = encode_felts_to_u32s(data.to_vec());

    // 2) Serialize the u32 limbs into a little-endian byte stream.
    let mut byte_stream = Vec::with_capacity(u32_words.len() * 4);
    for word in u32_words {
        byte_stream.extend_from_slice(&word.to_le_bytes());
    }

    // 3) Compute Blake2s-256 over the bytes and pack the result into a Felt.
    blake2s_to_felt(&byte_stream)
}
