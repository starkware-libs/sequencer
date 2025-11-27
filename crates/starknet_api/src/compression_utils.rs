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
    #[error("Decompressed data exceeds maximum size limit of {limit} bytes")]
    SizeLimitExceeded { limit: usize },
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

/// Decompresses the provided data with size limits.
fn decompress_with_size_limit(
    decoded_data: Vec<u8>,
    max_size: usize,
) -> Result<Vec<u8>, CompressionError> {
    let decompressor = flate2::read::GzDecoder::new(&decoded_data[..]);
    let mut decompressed_data = Vec::new();
    decompressor
        .take((max_size + 1).try_into().expect("max_size should be less than usize::MAX"))
        .read_to_end(&mut decompressed_data)?;
    if decompressed_data.len() > max_size {
        return Err(CompressionError::SizeLimitExceeded { limit: max_size });
    }
    Ok(decompressed_data)
}

/// Decodes the provided data with size limits.
// TODO(dan): consider limiting the time it takes to decompress.
pub fn decode_and_decompress_with_size_limit<T: DeserializeOwned>(
    value: &str,
    max_size: usize,
) -> Result<T, CompressionError> {
    let decoded_data = base64::decode(value)?;
    let decompressed_data = decompress_with_size_limit(decoded_data, max_size)?;
    Ok(serde_json::from_reader(decompressed_data.as_slice())?)
}
