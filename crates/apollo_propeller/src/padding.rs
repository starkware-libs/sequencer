//! Message padding utilities for erasure coding.
//!
//! This module provides functions to pad and unpad messages to ensure they are
//! evenly divisible by the number of data shards used in erasure coding.

use crate::types::ReconstructionError;

/// Pad a message to be evenly divisible by data shards.
///
/// Adds a 4-byte length prefix followed by zero padding to make the total
/// message length evenly divisible by `2 * num_data_shards`.
pub fn pad_message(message: Vec<u8>, num_data_shards: usize) -> Vec<u8> {
    let original_message_length: u32 = message.len().try_into().expect("Message length too long");
    let amount_to_pad = 2 * num_data_shards - ((message.len() + 4) % (2 * num_data_shards));
    [original_message_length.to_le_bytes().to_vec(), message, vec![0; amount_to_pad]].concat()
}

/// Remove padding from a message.
///
/// Reads the 4-byte length prefix and extracts the original message.
pub fn un_pad_message(message: Vec<u8>) -> Result<Vec<u8>, ReconstructionError> {
    if message.len() < 4 {
        return Err(ReconstructionError::MessagePaddingError);
    }
    let length_bytes: [u8; 4] = message[..4].try_into().expect("This should never fail");
    let original_message_length: u32 = u32::from_le_bytes(length_bytes);
    let original_message_length_usize: usize = original_message_length.try_into().unwrap();
    if 4 + original_message_length_usize > message.len() {
        return Err(ReconstructionError::MessagePaddingError);
    }
    Ok(message[4..(4 + original_message_length_usize)].to_vec())
}
