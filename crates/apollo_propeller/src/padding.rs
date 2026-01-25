//! Message padding utilities for erasure coding.
//!
//! This module provides functions to pad and unpad messages to ensure they are
//! evenly divisible by the number of data shards used in erasure coding.

use thiserror::Error;

use crate::types::ReconstructionError;

/// Errors that can occur during message unpadding.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UnpaddingError {
    #[error("Failed to decode varint length prefix: {0} (message length: {1})")]
    InvalidVarintLengthPrefix(String, usize),
    #[error(
        "Message length mismatch: expected {expected} bytes (prefix: {prefix} + content: \
         {content}), got {actual}"
    )]
    MessageLengthMismatch { expected: usize, prefix: usize, content: usize, actual: usize },
}

/// Pad a message to be evenly divisible by a given divisor.
///
/// Adds a varint-encoded length prefix followed by zero padding to make the total
/// message length (including the length prefix) evenly divisible by `divisor`.
///
/// # Arguments
///
/// * `message` - The message to pad
/// * `divisor` - The number the output length must be divisible by (must be > 0)
pub fn pad_message(message: Vec<u8>, divisor: usize) -> Vec<u8> {
    let original_message_length = message.len();

    // Encode the length as a varint
    let mut length_prefix = unsigned_varint::encode::usize_buffer();
    let length_bytes = unsigned_varint::encode::usize(original_message_length, &mut length_prefix);
    let varint_len = length_bytes.len();

    // Calculate padding needed to make (varint_len + message_len + padding) divisible by divisor
    let total_len_before_padding = varint_len + original_message_length;
    let amount_to_pad = (divisor - (total_len_before_padding % divisor)) % divisor;

    // Concatenate: length_prefix + message + padding
    [length_bytes, &message[..], &vec![0u8; amount_to_pad][..]].concat()
}

/// Remove padding from a message.
///
/// Reads the varint-encoded length prefix and extracts the original message.
pub fn unpad_message(message: Vec<u8>) -> Result<Vec<u8>, ReconstructionError> {
    // Decode the varint length prefix
    let (original_message_length, varint_prefix_len) =
        match unsigned_varint::decode::usize(&message) {
            Ok((length, remaining)) => {
                let prefix_len = message.len() - remaining.len();
                (length, prefix_len)
            }
            Err(e) => {
                let err = UnpaddingError::InvalidVarintLengthPrefix(e.to_string(), message.len());
                return Err(ReconstructionError::MessagePaddingError(err));
            }
        };

    if original_message_length > message.len().saturating_sub(varint_prefix_len) {
        let err = UnpaddingError::MessageLengthMismatch {
            expected: varint_prefix_len.saturating_add(original_message_length),
            prefix: varint_prefix_len,
            content: original_message_length,
            actual: message.len(),
        };
        return Err(ReconstructionError::MessagePaddingError(err));
    }

    // Extract the original message (skip the varint prefix and any padding)
    Ok(message[varint_prefix_len..varint_prefix_len + original_message_length].to_vec())
}
