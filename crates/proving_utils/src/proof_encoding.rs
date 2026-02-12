use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use bzip2::read::BzDecoder;
use bzip2::write::BzEncoder;
use bzip2::Compression;
use starknet_api::transaction::fields::Proof;
use thiserror::Error;

/// Errors that can occur during proof encoding/decoding operations.
#[derive(Debug, Error)]
pub enum ProofEncodingError {
    /// The encoded data is empty (missing padding prefix).
    #[error("Encoded data is empty")]
    EmptyData,
    /// The padding value in the prefix is invalid (must be 0-3).
    #[error("Invalid padding value {0} (must be 0-3)")]
    InvalidPadding(u32),
    /// Non-zero padding was specified but no packed data was provided.
    #[error("Non-zero padding with no packed data")]
    PaddingWithoutData,
    /// An I/O error occurred during file operations.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Encodes bytes into `u32` values with a padding prefix.
///
/// The first element stores the number of padding bytes (0-3) added at the beginning.
/// The remaining elements store the data packed as big-endian `u32` values, with padding
/// zeros prepended to align the data to a 4-byte boundary.
///
/// The original length can be reconstructed as: `(packed.len() - 1) * 4 - padding`.
pub fn encode_bytes_to_u32(data: &[u8]) -> Vec<u32> {
    let remainder = data.len() % 4;
    let padding = (4 - remainder) % 4;
    let padded_len = data.len() + padding;
    let mut encoded = Vec::with_capacity(1 + padded_len / 4);
    let padding_u32 = u32::try_from(padding).expect("Padding fits in u32.");
    encoded.push(padding_u32);

    if remainder > 0 {
        // Create padded chunk with zeros at the beginning.
        let mut padded_chunk = vec![0u8; 4];
        padded_chunk[padding..].copy_from_slice(&data[..remainder]);
        encoded.push(u32::from_be_bytes(
            padded_chunk.try_into().expect("Padded chunk must be 4 bytes"),
        ));
    }

    for chunk in data[remainder..].chunks(4) {
        encoded.push(u32::from_be_bytes(chunk.try_into().expect("Chunk must be 4 bytes")));
    }

    encoded
}

/// Decodes `u32` values into bytes, using the padding prefix to skip leading zeros.
///
/// The first element must contain the number of padding bytes (0-3). The remaining elements
/// contain the packed data as big-endian `u32` values, with padding zeros at the beginning.
pub fn decode_u32_to_bytes(data: &[u32]) -> Result<Vec<u8>, ProofEncodingError> {
    let (&padding, packed) = data.split_first().ok_or(ProofEncodingError::EmptyData)?;

    if padding > 3 {
        return Err(ProofEncodingError::InvalidPadding(padding));
    }

    // Non-zero padding requires at least one data element.
    if padding != 0 && packed.is_empty() {
        return Err(ProofEncodingError::PaddingWithoutData);
    }

    let padding = usize::try_from(padding).expect("Padding fits in usize.");
    let mut bytes = Vec::with_capacity(packed.len() * 4 - padding);
    if !packed.is_empty() {
        // Add the first element, skipping the padding bytes.
        bytes.extend_from_slice(&packed[0].to_be_bytes()[padding..]);

        // Add the remaining elements in full.
        for &value in &packed[1..] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
    }

    Ok(bytes.to_vec())
}

/// Raw proof bytes, convertible to/from the packed `Proof` representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofBytes(pub Vec<u8>);

impl ProofBytes {
    /// Reads and decompresses proof bytes from a bzip2-compressed file.
    /// Based on cairo_air::utils::deserialize_proof_from_file.
    pub fn from_file(path: &Path) -> Result<Self, ProofEncodingError> {
        let file = File::open(path)?;
        let mut bytes = Vec::new();
        let mut bz_decoder = BzDecoder::new(file);
        bz_decoder.read_to_end(&mut bytes)?;
        Ok(Self(bytes))
    }

    /// Compresses and writes proof bytes to a bzip2-compressed file.
    /// Based on cairo_air::utils::serialize_proof_to_file.
    pub fn to_file(&self, path: &Path) -> Result<(), ProofEncodingError> {
        let file = File::create(path)?;
        let mut encoder = BzEncoder::new(file, Compression::best());
        encoder.write_all(&self.0)?;
        encoder.finish()?;
        Ok(())
    }
}

impl TryFrom<Proof> for ProofBytes {
    type Error = ProofEncodingError;

    fn try_from(proof: Proof) -> Result<Self, Self::Error> {
        let bytes = decode_u32_to_bytes(&proof.0)?;
        Ok(Self(bytes))
    }
}

impl From<ProofBytes> for Proof {
    fn from(proof_bytes: ProofBytes) -> Self {
        encode_bytes_to_u32(&proof_bytes.0).into()
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::empty(vec![])]
    #[case::single_byte(vec![0x42])]
    #[case::aligned_4_bytes(vec![0x01, 0x02, 0x03, 0x04])]
    #[case::unaligned_6_bytes(vec![0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF])]
    fn test_proof_bytes_to_proof_round_trip(#[case] data: Vec<u8>) {
        let original_proof = ProofBytes(data);
        let proof: Proof = original_proof.clone().into();
        let recovered: ProofBytes = proof.try_into().unwrap();
        assert_eq!(original_proof, recovered);
    }

    #[rstest]
    #[case::empty(vec![])]
    #[case::small(vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09])]
    #[case::large((0u8..=255).cycle().take(10000).collect())]
    fn test_proof_bytes_file_round_trip(#[case] data: Vec<u8>) {
        let original_proof = ProofBytes(data);
        let temp_file = tempfile::NamedTempFile::new().unwrap();
        let path = temp_file.path();

        original_proof.to_file(path).unwrap();
        let recovered = ProofBytes::from_file(path).unwrap();

        assert_eq!(original_proof, recovered);
    }
}
