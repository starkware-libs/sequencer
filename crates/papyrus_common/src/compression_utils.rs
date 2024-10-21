use std::io::Read;

#[cfg(test)]
#[path = "compression_utils_test.rs"]
mod compression_utils_test;

#[derive(thiserror::Error, Debug)]
pub enum CompressionUtilsError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    Decode(#[from] base64::DecodeError),
}

// Compress the value using gzip with the default compression level and encode it in base64.
pub fn compress_and_encode(value: serde_json::Value) -> Result<String, CompressionUtilsError> {
    let mut compressor = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    serde_json::to_writer(&mut compressor, &value)?;
    let compressed_data = compressor.finish()?;
    Ok(base64::encode(compressed_data))
}

// Decompress the value from base64 and gzip.
pub fn decode_and_decompress(value: &str) -> Result<serde_json::Value, CompressionUtilsError> {
    let decoded_data = base64::decode(value)?;
    let mut decompressor = flate2::read::GzDecoder::new(&decoded_data[..]);
    let mut decompressed_data = String::new();
    decompressor.read_to_string(&mut decompressed_data)?;
    let json_value = serde_json::from_str(&decompressed_data)?;
    Ok(json_value)
}
