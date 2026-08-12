use std::collections::VecDeque;

use apollo_protobuf::protobuf::PropellerUnit as ProtoUnit;
use prost::encoding::encoded_len_varint;
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
    let unit_encoded_len_u64 = u64::try_from(unit_encoded_len).expect("encoded length fits in u64");
    // 1-byte tag (field 1, LEN wire type 0x0A) + varint-encoded item length + item bytes.
    1 + encoded_len_varint(unit_encoded_len_u64) + unit_encoded_len
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
    // Make items whose individual cost is known, then cap the batch to fit exactly 2.
    let unit = make_proto_unit(20);
    let single_item_cost = item_batch_cost(&unit);
    let max_size = 2 * single_item_cost + 1; // fits exactly 2, 3rd would exceed

    let num_items = 5usize;
    let mut queue: VecDeque<ProtoUnit> = (0..num_items).map(|_| unit.clone()).collect();

    let batch = Handler::create_message_batch(&mut queue, max_size);

    assert_eq!(batch.batch.len(), 2, "should pack exactly 2 items");
    assert_eq!(queue.len(), 3, "3 items should remain in the queue");
    assert!(batch.encoded_len() <= max_size);
}

#[test]
fn test_create_message_batch_varint_boundary() {
    // PropellerUnit.signature is field 6 (bytes, 1-byte tag 0x32).
    // make_proto_unit(n).encoded_len() = 1 (tag) + varint_len(n) + n for n < 128.
    // At the encoded_len 127->128 boundary, the per-item length varint grows from 1 to 2 bytes:
    //   encoded_len = 127: item_batch_cost = 1 + 1 + 127 = 129
    //   encoded_len = 128: item_batch_cost = 1 + 2 + 128 = 131
    // Verify that the unit at the boundary is excluded when the budget fits a 129-byte item
    // but not a 131-byte one.
    let first_unit = make_proto_unit(10);
    let below_boundary_unit = make_proto_unit(125); // encoded_len = 127, 1-byte varint
    let at_boundary_unit = make_proto_unit(126); // encoded_len = 128, 2-byte varint

    assert_eq!(below_boundary_unit.encoded_len(), 127);
    assert_eq!(at_boundary_unit.encoded_len(), 128);

    let first_cost = item_batch_cost(&first_unit);
    let below_boundary_cost = item_batch_cost(&below_boundary_unit); // 129
    let at_boundary_cost = item_batch_cost(&at_boundary_unit); // 131

    // Budget: fits first_unit + a below-boundary item (129 bytes), but not first_unit +
    // at_boundary_unit (131 bytes — 2 bytes more due to the 2-byte varint).
    let max_size = first_cost + below_boundary_cost;
    assert!(max_size < first_cost + at_boundary_cost, "at_boundary_unit must not fit");

    let mut queue = VecDeque::from([first_unit, at_boundary_unit]);
    let batch = Handler::create_message_batch(&mut queue, max_size);

    assert_eq!(batch.batch.len(), 1, "2-byte varint causes at_boundary_unit to exceed budget");
    assert_eq!(queue.len(), 1, "at_boundary_unit remains in queue");
    assert!(batch.encoded_len() <= max_size);
}
