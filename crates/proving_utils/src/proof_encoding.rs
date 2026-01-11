use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

/// Reads a binary file and encodes it into a `Vec<u32>`.
pub fn encode_binary_file_to_u32(path: &Path) -> std::io::Result<Vec<u32>> {
    let mut file = File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(encode_bytes_to_u32(&buffer))
}

/// Writes a `Vec<u32>` to a binary file by decoding it into bytes.
pub fn decode_u32_to_binary_file(path: &Path, data: &[u32]) -> std::io::Result<()> {
    let bytes = decode_u32_to_bytes(data)?;
    let mut file = File::create(path)?;
    file.write_all(&bytes)?;
    Ok(())
}

/// Encodes bytes into `u32` values with a length prefix.
///
/// Note: This encoding is limited to files of size at most `u32::MAX` bytes.
pub fn encode_bytes_to_u32(data: &[u8]) -> Vec<u32> {
    let mut encoded = Vec::with_capacity(1 + data.len().div_ceil(4));
    // Lengths larger than u32::MAX are rejected by this encoder.
    encoded.push(u32::try_from(data.len()).expect("Encoded data length must fit into u32."));

    for chunk in data.chunks(4) {
        let mut bytes = [0u8; 4];
        bytes[..chunk.len()].copy_from_slice(chunk);
        encoded.push(u32::from_be_bytes(bytes));
    }

    encoded
}

/// Decodes `u32` values into bytes, preserving original data length.
pub fn decode_u32_to_bytes(data: &[u32]) -> std::io::Result<Vec<u8>> {
    let (&length, packed) = data.split_first().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "Encoded data is empty.")
    })?;
    let length = usize::try_from(length)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "Length overflow."))?;
    let expected_bytes = packed.len().saturating_mul(4);
    if length > expected_bytes {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Encoded length exceeds packed data size.",
        ));
    }

    let mut bytes = Vec::with_capacity(expected_bytes);
    for &value in packed {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    bytes.truncate(length);
    Ok(bytes)
}
