use apollo_protobuf::protobuf::{PropellerUnit, PropellerUnitBatch};
use asynchronous_codec::{Decoder, Encoder};
use bytes::BytesMut;
use rstest::{fixture, rstest};

use crate::codec::ProstCodec;

#[fixture]
fn codec() -> ProstCodec<PropellerUnitBatch> {
    ProstCodec::new(10 * 1024 * 1024) // 10MB to handle large test
}

#[fixture]
fn batch1() -> PropellerUnitBatch {
    PropellerUnitBatch {
        batch: vec![
            PropellerUnit { shard: vec![1, 2, 3], ..Default::default() },
            PropellerUnit { shard: vec![4, 5], ..Default::default() },
        ],
    }
}

#[fixture]
fn batch2() -> PropellerUnitBatch {
    PropellerUnitBatch {
        batch: vec![PropellerUnit { shard: vec![6, 7, 8, 9], ..Default::default() }],
    }
}

#[fixture]
fn batch3() -> PropellerUnitBatch {
    PropellerUnitBatch {
        batch: vec![
            PropellerUnit { shard: vec![10], ..Default::default() },
            PropellerUnit { shard: vec![11, 12], ..Default::default() },
            PropellerUnit { shard: vec![13, 14, 15], ..Default::default() },
        ],
    }
}

fn encode_batch(
    codec: &mut ProstCodec<PropellerUnitBatch>,
    batch: &PropellerUnitBatch,
) -> BytesMut {
    let mut buf = BytesMut::new();
    codec.encode(batch.clone(), &mut buf).expect("encoding should succeed");
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
fn test_roundtrip(mut codec: ProstCodec<PropellerUnitBatch>, #[case] data_size: usize) {
    let batch = PropellerUnitBatch {
        batch: vec![PropellerUnit { shard: vec![0xAB; data_size], ..Default::default() }],
    };

    let mut buf = encode_batch(&mut codec, &batch);

    let decoded = codec.decode(&mut buf).expect("decoding should succeed");
    assert_eq!(decoded, Some(batch));
    assert!(buf.is_empty());
}

#[rstest]
fn test_empty_buffer(mut codec: ProstCodec<PropellerUnitBatch>) {
    let mut buf = BytesMut::new();

    let result = codec.decode(&mut buf).expect("decoding should not error");
    assert_eq!(result, None);
}

#[rstest]
fn test_multiple_messages_byte_by_byte(
    mut codec: ProstCodec<PropellerUnitBatch>,
    batch1: PropellerUnitBatch,
    batch2: PropellerUnitBatch,
    batch3: PropellerUnitBatch,
) {
    // Encode all batches into one buffer
    let mut full_buf = BytesMut::new();
    codec.encode(batch1.clone(), &mut full_buf).expect("encoding should succeed");
    codec.encode(batch2.clone(), &mut full_buf).expect("encoding should succeed");
    codec.encode(batch3.clone(), &mut full_buf).expect("encoding should succeed");

    let total_len = full_buf.len();
    let mut partial_buf = BytesMut::new();
    let mut decoded_batches = Vec::new();

    // Add bytes one at a time and try to decode after each addition
    for i in 0..total_len {
        partial_buf.extend_from_slice(&full_buf[i..i + 1]);

        // Try to decode as many batches as possible
        while let Ok(Some(batch)) = codec.decode(&mut partial_buf) {
            decoded_batches.push(batch);
        }
    }

    // Verify we got all three batches in correct order
    assert_eq!(decoded_batches.len(), 3);
    assert_eq!(decoded_batches[0], batch1);
    assert_eq!(decoded_batches[1], batch2);
    assert_eq!(decoded_batches[2], batch3);
    assert!(partial_buf.is_empty());
}

#[test]
fn test_max_length_exceeded() {
    let mut codec = ProstCodec::<PropellerUnitBatch>::new(10);
    let batch = PropellerUnitBatch {
        batch: vec![PropellerUnit { shard: vec![1; 100], ..Default::default() }],
    };

    // Encoding should fail
    let mut buf = BytesMut::new();
    let result = codec.encode(batch.clone(), &mut buf);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::PermissionDenied);

    // Decoding should also fail when message exceeds limit
    let mut large_codec = ProstCodec::<PropellerUnitBatch>::new(1024);
    let mut large_buf = encode_batch(&mut large_codec, &batch);

    let result = codec.decode(&mut large_buf);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::PermissionDenied);
}
