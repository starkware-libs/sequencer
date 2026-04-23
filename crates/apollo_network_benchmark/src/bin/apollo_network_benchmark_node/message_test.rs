use rstest::rstest;

use crate::message::{StressTestMessage, METADATA_SIZE};

#[rstest]
#[case::small_payload(10)]
#[case::medium_payload(300)]
#[case::large_payload(20_000)]
fn message_len_matches_serialized_size(#[case] payload_len: usize) {
    let payload = vec![0xAA; payload_len];
    let message = StressTestMessage::new(1, 7070, payload.clone());
    let expected_size = *METADATA_SIZE + payload_len;
    assert_eq!(message.len(), expected_size);
    assert_eq!(message.slow_len(), expected_size);
}

#[rstest]
#[case::extreme_metadata_values(
    u64::MAX - 1,
    u64::MAX - 2,
    vec![0xa1, 0xb2, 0xc3, 0xd4, 0xe5],
)]
#[case::empty_payload(1, 42, vec![])]
fn round_trip_preserves_all_fields(
    #[case] sender_id: u64,
    #[case] message_index: u64,
    #[case] payload: Vec<u8>,
) {
    let original_message = StressTestMessage::new(sender_id, message_index, payload);

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
fn truncated_header_returns_parse_error() {
    let error = StressTestMessage::try_from(vec![0u8; 5]).unwrap_err();
    assert!(error.to_string().contains("truncated"), "got: {error}");
}

#[test]
fn truncated_payload_returns_parse_error() {
    let message = StressTestMessage::new(1, 2, vec![0xAA; 100]);
    let mut serialized: Vec<u8> = message.into();
    serialized.truncate(serialized.len() - 10);
    let error = StressTestMessage::try_from(serialized).unwrap_err();
    assert!(error.to_string().contains("truncated"), "got: {error}");
}
