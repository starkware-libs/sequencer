use asynchronous_codec::{Decoder, Encoder};
use bytes::BytesMut;
use prost::Message;
use rstest::rstest;

use crate::codec::ProtoCodec;

const DEFAULT_MAX_MESSAGE_LEN_BYTES: usize = 1 << 16;

#[derive(Clone, PartialEq, Message)]
struct TestMessage {
    #[prost(bytes = "vec", tag = "1")]
    data: Vec<u8>,
}

fn codec(max_message_len_bytes: usize) -> ProtoCodec<TestMessage> {
    ProtoCodec::new(max_message_len_bytes)
}

fn test_message(i: u8) -> TestMessage {
    TestMessage { data: vec![0xAA ^ i; 1 << i] }
}

fn encode_message(codec: &mut ProtoCodec<TestMessage>, message: &TestMessage) -> BytesMut {
    let mut buf = BytesMut::new();
    codec.encode(message.clone(), &mut buf).unwrap();
    buf
}

#[rstest]
#[case(0)]
#[case(127)]
#[case(128)]
#[case(255)]
#[case(256)]
#[case(1000)]
#[case(10000)]
#[case(100_000)]
fn test_encode_decode_various_sizes(#[case] data_size: usize) {
    let message = TestMessage { data: vec![0xAB; data_size] };
    let mut test_codec = codec(message.encoded_len());
    let mut buf = encode_message(&mut test_codec, &message);
    let decoded = test_codec.decode(&mut buf).unwrap();
    assert_eq!(decoded, Some(message));
    assert!(buf.is_empty());
}

#[test]
fn test_empty_buffer() {
    let mut test_codec = codec(DEFAULT_MAX_MESSAGE_LEN_BYTES);
    let mut buf = BytesMut::new();

    let result = test_codec.decode(&mut buf).unwrap();
    assert_eq!(result, None);
}

#[test]
fn test_multiple_messages_byte_by_byte() {
    const NUM_MESSAGES_IN_STREAM_TEST: u8 = 20;
    // Need larger max to accommodate test_message(20) which is 1 << 20 bytes
    let mut test_codec = codec(1 << (NUM_MESSAGES_IN_STREAM_TEST + 1));
    // Encode all messages into one buffer
    let mut full_buf = BytesMut::new();
    let sent_messages = (1..=NUM_MESSAGES_IN_STREAM_TEST).map(test_message).collect::<Vec<_>>();
    for message in sent_messages.iter() {
        test_codec.encode(message.clone(), &mut full_buf).unwrap();
    }

    let total_len = full_buf.len();
    let mut partial_buf = BytesMut::new();
    let mut decoded_messages = Vec::new();

    // Add bytes one at a time and try to decode after each addition
    for i in 0..total_len {
        partial_buf.extend_from_slice(&full_buf[i..i + 1]);

        // Try to decode as many messages as possible
        while let Ok(Some(message)) = test_codec.decode(&mut partial_buf) {
            decoded_messages.push(message);
        }
    }

    // Verify we got all messages in correct order
    assert_eq!(decoded_messages.len(), sent_messages.len());
    for (i, message) in decoded_messages.iter().enumerate() {
        assert_eq!(message, &sent_messages[i]);
    }
    assert!(partial_buf.is_empty());
}

#[test]
fn test_max_length_exceeded() {
    let mut small_codec = codec(10);
    let message = TestMessage { data: vec![1; 100] };

    // Encoding should fail
    let mut buf = BytesMut::new();
    let result = small_codec.encode(message.clone(), &mut buf);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidInput);

    // Decoding should also fail when message exceeds limit
    let mut large_codec = codec(DEFAULT_MAX_MESSAGE_LEN_BYTES);
    let mut large_buf = encode_message(&mut large_codec, &message);

    let result = small_codec.decode(&mut large_buf);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn test_invalid_varint_overflow() {
    let mut codec = codec(DEFAULT_MAX_MESSAGE_LEN_BYTES);

    // Create a varint that overflows: 10 bytes with all continuation bits set
    // This is invalid because a valid varint must terminate within 10 bytes
    let mut buf = BytesMut::new();
    buf.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);

    let result = codec.decode(&mut buf);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn test_incomplete_varint_returns_none() {
    let mut test_codec = codec(DEFAULT_MAX_MESSAGE_LEN_BYTES);

    // Incomplete varint: continuation bit set but no next byte
    let mut buf = BytesMut::new();
    buf.extend_from_slice(&[0x80]);

    let result = test_codec.decode(&mut buf);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), None); // Should wait for more data
}

#[test]
fn test_empty_buffer_returns_none() {
    let mut test_codec = codec(DEFAULT_MAX_MESSAGE_LEN_BYTES);
    let mut buf = BytesMut::new();

    let result = test_codec.decode(&mut buf);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), None);
}

#[rstest]
#[case(1)]
#[case(2)]
#[case(3)]
#[case(10)]
#[case(100)]
#[case(101)]
#[case(102)]
#[case(103)]
fn test_incomplete_message_returns_none(#[case] bytes_to_remove: usize) {
    let mut test_codec = codec(DEFAULT_MAX_MESSAGE_LEN_BYTES);
    let message = TestMessage { data: vec![0xAB; 100] };
    let mut full_buf = encode_message(&mut test_codec, &message);

    // Take only part of the buffer
    let partial_len = full_buf.len() - bytes_to_remove;
    let mut partial_buf = full_buf.split_to(partial_len);

    let result = test_codec.decode(&mut partial_buf);

    assert_eq!(result.unwrap(), None);
    assert_eq!(partial_buf.len(), partial_len);
}
