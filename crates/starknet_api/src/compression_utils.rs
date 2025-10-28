use std::io::Read;

use serde::de::DeserializeOwned;

#[cfg(test)]
#[path = "compression_utils_test.rs"]
mod compression_utils_test;

#[derive(thiserror::Error, Debug)]
pub enum CompressionError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    Decode(#[from] base64::DecodeError),
    #[error("Decompression limit exceeded")]
    DecompressionLimitExceeded,
}

/// Compress the value using gzip with the default compression level and encode it in base64.
pub fn compress_and_encode<T>(value: &T) -> Result<String, std::io::Error>
where
    T: ?Sized + serde::Serialize,
{
    let mut compressor = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    serde_json::to_writer(&mut compressor, value)?;
    let compressed_data = compressor.finish()?;
    Ok(base64::encode(compressed_data))
}

/// Decompress the value from base64 and gzip.
pub fn decode_and_decompress<T: DeserializeOwned>(
    value: &str,
    limit: u64,
) -> Result<T, CompressionError> {
    let decoded_data = base64::decode(value)?;
    let mut decompressor = flate2::read::GzDecoder::new(&decoded_data[..]).take(limit);
    let mut decompressed_data = String::new();
    let bytes_read = decompressor.read_to_string(&mut decompressed_data)?;
    if bytes_read == limit as usize {
        return Err(CompressionError::DecompressionLimitExceeded);
    }

    Ok(serde_json::from_str(&decompressed_data)?)
}
