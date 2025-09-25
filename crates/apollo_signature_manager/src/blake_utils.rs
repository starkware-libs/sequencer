use blake2::Blake2s256;
use digest::Digest;
use starknet_crypto::Felt;

/// Packs the first 8 little-endian 32-bit words (32 bytes) of `bytes`
/// into a single 252-bit Felt.
fn pack_256_le_to_felt(bytes: &[u8]) -> Felt {
    assert!(bytes.len() >= 32, "need at least 32 bytes to pack 8 words");

    // 1) copy your 32-byte LE-hash into the low 32 bytes of a 32-byte buffer.
    let mut buf = [0u8; 32];
    buf[..32].copy_from_slice(&bytes[..32]);

    // 2) interpret the whole 32-byte buffer as a little-endian Felt.
    Felt::from_bytes_le(&buf)
}

pub(crate) fn blake2s_to_felt(data: &[u8]) -> Felt {
    let mut hasher = Blake2s256::new();
    hasher.update(data);
    let hash32 = hasher.finalize();
    pack_256_le_to_felt(hash32.as_slice())
}
