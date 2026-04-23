use rstest::rstest;

use crate::message::{StressTestMessage, METADATA_SIZE};

#[rstest]
#[case::one_byte_len(10)]
#[case::two_byte_len(300)]
#[case::three_byte_len(20_000)]
fn test_message_size(#[case] vec_len: usize) {
    let payload = vec![0xAA; vec_len];
    let message = StressTestMessage::new(1, 7070, payload.clone());
    let expected_size = *METADATA_SIZE + vec_len;
    assert_eq!(message.len(), expected_size);
    assert_eq!(message.slow_len(), expected_size);
}

#[test]
fn test_serialization_and_deserialization() {
    let original_message =
        StressTestMessage::new(u64::MAX - 1, u64::MAX - 2, vec![0xa1, 0xb2, 0xc3, 0xd4, 0xe5]);

    let serialized_bytes: Vec<u8> = original_message.clone().into();
    let deserialized_message = StressTestMessage::try_from(serialized_bytes).unwrap();

    assert_eq!(deserialized_message.metadata.sender_id, original_message.metadata.sender_id);
    assert_eq!(
        deserialized_message.metadata.message_index,
        original_message.metadata.message_index
    );
    assert_eq!(deserialized_message.payload, original_message.payload);
    assert_eq!(deserialized_message.metadata.time, original_message.metadata.time);
}

#[test]
fn test_empty_payload() {
    let original_message = StressTestMessage::new(1, 42, vec![]);

    let serialized_bytes: Vec<u8> = original_message.clone().into();
    let deserialized_message = StressTestMessage::try_from(serialized_bytes).unwrap();

    assert_eq!(deserialized_message.metadata.sender_id, original_message.metadata.sender_id);
    assert_eq!(
        deserialized_message.metadata.message_index,
        original_message.metadata.message_index
    );
    assert_eq!(deserialized_message.payload, original_message.payload);
    assert_eq!(deserialized_message.metadata.time, original_message.metadata.time);
}

#[test]
fn test_truncated_header_rejected() {
    let error = StressTestMessage::try_from(vec![0u8; 5]).unwrap_err();
    assert!(error.to_string().contains("truncated"), "got: {error}");
}

#[test]
fn test_truncated_payload_rejected() {
    let message = StressTestMessage::new(1, 2, vec![0xAA; 100]);
    let mut serialized: Vec<u8> = message.into();
    serialized.truncate(serialized.len() - 10);
    let error = StressTestMessage::try_from(serialized).unwrap_err();
    assert!(error.to_string().contains("truncated"), "got: {error}");
}
