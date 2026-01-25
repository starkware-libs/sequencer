use rstest::*;

use crate::padding::{pad_message, un_pad_message};
use crate::types::ReconstructionError;

#[rstest]
#[case(vec![1, 2, 3], 2)]
#[case(vec![1, 2, 3, 4, 5], 3)]
#[case(vec![42; 100], 10)]
fn test_pad_and_unpad_message(#[case] message: Vec<u8>, #[case] num_data_shards: usize) {
    let padded = pad_message(message.clone(), num_data_shards);
    assert_eq!(padded.len() % (2 * num_data_shards), 0);
    let unpadded = un_pad_message(padded).expect("Failed to unpad message");
    assert_eq!(unpadded, message);
}

#[rstest]
fn test_unpad_message_too_short() {
    let result = un_pad_message(vec![1, 2]);
    assert!(matches!(result, Err(ReconstructionError::MessagePaddingError)));
}

#[rstest]
fn test_unpad_message_invalid_length() {
    let mut invalid_message = vec![0u8; 10];
    invalid_message[0..4].copy_from_slice(&100u32.to_le_bytes());
    let result = un_pad_message(invalid_message);
    assert!(matches!(result, Err(ReconstructionError::MessagePaddingError)));
}
