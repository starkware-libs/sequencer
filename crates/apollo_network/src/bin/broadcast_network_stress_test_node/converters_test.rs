use super::*;

#[test]
fn test_byte_size() {
    let payload = vec![0xAA; 10];
    let message = StressTestMessage::new(1, 7070, payload.clone());

    // Calculate expected size: sender_id + message_index + time + payload_len + payload
    let expected_size = *METADATA_SIZE + payload.len();

    assert_eq!(message.byte_size(), expected_size);
}

#[test]
fn test_serialization_and_deserilization() {
    let original_message =
        StressTestMessage::new(u64::MAX - 1, u64::MAX - 2, vec![0xa1, 0xb2, 0xc3, 0xd4, 0xe5]);

    // Serialize to bytes
    let serialized_bytes: Vec<u8> = original_message.clone().into();

    // Deserialize back to message
    let deserialized_message: StressTestMessage = serialized_bytes.into();

    // Verify all fields match
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
    let deserialized_message: StressTestMessage = serialized_bytes.into();

    assert_eq!(deserialized_message.metadata.sender_id, original_message.metadata.sender_id);
    assert_eq!(
        deserialized_message.metadata.message_index,
        original_message.metadata.message_index
    );
    assert_eq!(deserialized_message.payload, original_message.payload);
    assert_eq!(deserialized_message.metadata.time, original_message.metadata.time);
}
