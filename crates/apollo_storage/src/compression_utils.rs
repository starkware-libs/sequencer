#[cfg(test)]
#[path = "compression_utils_test.rs"]
mod compression_utils_test;

use std::cell::RefCell;

use zstd::bulk::{Compressor, Decompressor};

use crate::db::serialization::{StorageSerde, StorageSerdeError};

// TODO(Dvir): fine tune the compression hyperparameters (and maybe even the compression
// algorithm).

// The maximum size of the decompressed data.
// TODO(Dvir): consider defining this for each type separately and pass it as an argument to the
// decompress function.
pub(crate) const MAX_DECOMPRESSED_SIZE: usize = 1 << 28; // 256 MB
// The compression level to use. Higher levels are slower but compress better.
const COMPRESSION_LEVEL: i32 = zstd::DEFAULT_COMPRESSION_LEVEL;

thread_local! {
    static COMPRESSOR: RefCell<Compressor<'static>> =
        RefCell::new(Compressor::new(COMPRESSION_LEVEL).expect("zstd compressor should be creatable (only fails on OOM)"));
    static DECOMPRESSOR: RefCell<Decompressor<'static>> =
        RefCell::new(Decompressor::new().expect("zstd decompressor should be creatable (only fails on OOM)"));
}

fn with_compressor<T>(
    f: impl FnOnce(&mut Compressor<'static>) -> std::io::Result<T>,
) -> std::io::Result<T> {
    COMPRESSOR.with(|cell| {
        let mut borrow = cell.try_borrow_mut().map_err(|err| {
            std::io::Error::other(format!(
                "zstd compressor: reentrant borrow on thread-local: {err}"
            ))
        })?;
        f(&mut borrow)
    })
}

fn with_decompressor<T>(
    f: impl FnOnce(&mut Decompressor<'static>) -> std::io::Result<T>,
) -> std::io::Result<T> {
    DECOMPRESSOR.with(|cell| {
        let mut borrow = cell.try_borrow_mut().map_err(|err| {
            std::io::Error::other(format!(
                "zstd decompressor: reentrant borrow on thread-local: {err}"
            ))
        })?;
        f(&mut borrow)
    })
}

/// Returns the compressed data in a vector.
///
/// Uses a thread-local compressor to avoid re-allocating the zstd context on every call.
///
/// # Arguments
/// * data - bytes to compress.
///
/// # Errors
/// Returns [`std::io::Error`] if any read error is encountered.
pub fn compress(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    with_compressor(|compressor| compressor.compress(data))
}

/// Serialized and then compress object.
///
/// # Arguments
/// * object - the object to serialize and compress.
///
/// # Errors
/// Returns [`StorageSerdeError`] if any error is encountered in the serialization or compression.
pub fn serialize_and_compress(object: &impl StorageSerde) -> Result<Vec<u8>, StorageSerdeError> {
    let mut buf = Vec::new();
    object.serialize_into(&mut buf)?;
    Ok(compress(buf.as_slice())?)
}

/// Decompress data and returns it as bytes in a vector.
///
/// Uses a thread-local decompressor to avoid re-allocating the zstd context on every call.
///
/// # Arguments
/// * data - bytes to decompress.
///
/// # Errors
/// Returns [`std::io::Error`] if any read error is encountered.
pub fn decompress(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    with_decompressor(|decompressor| decompressor.decompress(data, MAX_DECOMPRESSED_SIZE))
}

/// Decompress a vector directly from a reader.
/// In case of successful decompression, the vector will be returned; otherwise, None.
///
/// # Arguments
/// * bytes - bytes to read.
pub fn decompress_from_reader(bytes: &mut impl std::io::Read) -> Option<Vec<u8>> {
    let compressed_data = Vec::<u8>::deserialize_from(bytes)?;
    decompress(compressed_data.as_slice()).ok()
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum IsCompressed {
    No = 0,
    Yes = 1,
}
