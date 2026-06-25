use std::collections::VecDeque;

use apollo_protobuf::protobuf::PropellerUnit as ProtoUnit;
use prost::Message;

use super::Handler;

/// Build a `ProtoUnit` whose `signature` field is `payload_bytes` bytes long, giving
/// predictable and controllable encoded sizes.
fn make_proto_unit(payload_bytes: usize) -> ProtoUnit {
    ProtoUnit { signature: vec![0u8; payload_bytes], ..Default::default() }
}

/// Return the incremental cost of adding one more item to a `ProtoBatch`.
/// Matches the formula used in `create_message_batch`.
fn item_batch_cost(unit: &ProtoUnit) -> usize {
    let unit_encoded_len = unit.encoded_len();
    let unit_encoded_len_u64 =
        u64::try_from(unit_encoded_len).expect("encoded length fits in u64");
    1 + prost::encoding::encoded_len_varint(unit_encoded_len_u64) + unit_encoded_len
}

#[test]
fn test_create_message_batch_empty_queue() {
    let mut queue: VecDeque<ProtoUnit> = VecDeque::new();
    let batch = Handler::create_message_batch(&mut queue, 1024);
    assert!(batch.batch.is_empty());
    assert!(queue.is_empty());
}

#[test]
fn test_create_message_batch_single_item_fits() {
    let unit = make_proto_unit(10);
    let unit_cost = item_batch_cost(&unit);
    let mut queue = VecDeque::from([unit]);
    let batch = Handler::create_message_batch(&mut queue, unit_cost + 100);
    assert_eq!(batch.batch.len(), 1);
    assert!(queue.is_empty());
    assert!(batch.encoded_len() <= unit_cost + 100);
}

#[test]
fn test_create_message_batch_single_item_over_limit_still_included() {
    // The first item is always included (the oversized warning is purely advisory).
    let unit = make_proto_unit(200);
    let mut queue = VecDeque::from([unit]);
    let batch = Handler::create_message_batch(&mut queue, 1);
    assert_eq!(batch.batch.len(), 1);
    assert!(queue.is_empty());
}

#[test]
fn test_create_message_batch_all_items_fit() {
    let num_items = 5;
    let unit = make_proto_unit(10);
    let total_cost: usize = (0..num_items).map(|_| item_batch_cost(&unit)).sum();
    let mut queue: VecDeque<ProtoUnit> = (0..num_items).map(|_| unit.clone()).collect();

    let batch = Handler::create_message_batch(&mut queue, total_cost + 100);

    assert_eq!(batch.batch.len(), num_items);
    assert!(queue.is_empty());
    assert!(batch.encoded_len() <= total_cost + 100);
}

#[test]
fn test_create_message_batch_stops_at_size_limit() {
    // Make items whose individual cost is known, then cap the batch at exactly 2 items.
    let unit = make_proto_unit(20);
    let single_item_cost = item_batch_cost(&unit);
    let max_size = 2 * single_item_cost; // fits exactly 2

    let num_items = 5usize;
    let mut queue: VecDeque<ProtoUnit> = (0..num_items).map(|_| unit.clone()).collect();

    let batch = Handler::create_message_batch(&mut queue, max_size);

    assert_eq!(batch.batch.len(), 2, "should pack exactly 2 items");
    assert_eq!(queue.len(), 3, "3 items should remain in the queue");
    assert!(batch.encoded_len() <= max_size);
}

#[test]
fn test_create_message_batch_packed_maximally() {
    // Verify that we include as many items as possible (no premature stops).
    let unit = make_proto_unit(8);
    let single_cost = item_batch_cost(&unit);
    let num_items = 10usize;

    for limit in (single_cost..=num_items * single_cost).step_by(single_cost) {
        let expected = limit / single_cost;
        let mut queue: VecDeque<ProtoUnit> = (0..num_items).map(|_| unit.clone()).collect();
        let batch = Handler::create_message_batch(&mut queue, limit);
        assert_eq!(
            batch.batch.len(),
            expected,
            "limit={limit}: expected {expected} items, got {}",
            batch.batch.len()
        );
        assert!(batch.encoded_len() <= limit);
    }
}

#[test]
fn test_create_message_batch_encoded_len_matches_proto() {
    // The incremental size tracking must agree with prost's own encoded_len at every step.
    let unit = make_proto_unit(15);
    let num_items = 6usize;
    let large_limit = usize::MAX;
    let mut queue: VecDeque<ProtoUnit> = (0..num_items).map(|_| unit.clone()).collect();

    let batch = Handler::create_message_batch(&mut queue, large_limit);

    assert_eq!(batch.batch.len(), num_items);
    // Confirm the batch's actual encoded length is what prost reports (invariant check).
    let expected_len: usize = batch
        .batch
        .iter()
        .map(|u| {
            let enc_len = u.encoded_len();
            let enc_len_u64 = u64::try_from(enc_len).expect("encoded length fits in u64");
            1 + prost::encoding::encoded_len_varint(enc_len_u64) + enc_len
        })
        .sum();
    assert_eq!(batch.encoded_len(), expected_len);
}
