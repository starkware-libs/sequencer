use std::collections::VecDeque;

use apollo_protobuf::protobuf::{PropellerUnit as ProtoUnit, PropellerUnitBatch as ProtoBatch};
use prost::Message;

use super::framework::*;
use crate::handler::Handler;

fn make_proto_unit_with_shard(shard: Vec<u8>) -> ProtoUnit {
    ProtoUnit::from(make_test_unit_with_shard(shard))
}

#[test]
fn batch_empty_queue() {
    let mut queue = VecDeque::new();
    let batch = Handler::create_message_batch(&mut queue, MAX_WIRE_MESSAGE_SIZE);
    assert!(batch.batch.is_empty());
    assert!(queue.is_empty());
}

#[test]
fn batch_single_message() {
    let mut queue = VecDeque::new();
    queue.push_back(make_proto_unit_with_shard(vec![1, 2, 3]));
    let batch = Handler::create_message_batch(&mut queue, MAX_WIRE_MESSAGE_SIZE);
    assert_eq!(batch.batch.len(), 1);
    assert!(queue.is_empty());
}

#[test]
fn batch_multiple_messages_within_limit() {
    let mut queue = VecDeque::new();
    for i in 0..5u8 {
        queue.push_back(make_proto_unit_with_shard(vec![i; 10]));
    }
    let batch = Handler::create_message_batch(&mut queue, MAX_WIRE_MESSAGE_SIZE);
    assert_eq!(batch.batch.len(), 5);
    assert!(queue.is_empty());
}

#[test]
fn batch_splits_at_size_limit() {
    let mut queue = VecDeque::new();
    for i in 0..5u8 {
        queue.push_back(make_proto_unit_with_shard(vec![i; 100]));
    }

    // Use a max size that can fit ~2 units but not all 5
    let single_unit_size = {
        let test_batch = ProtoBatch { batch: vec![make_proto_unit_with_shard(vec![0; 100])] };
        test_batch.encoded_len()
    };
    let max_size = single_unit_size * 2 + 50;

    let batch = Handler::create_message_batch(&mut queue, max_size);
    assert!(!batch.batch.is_empty());
    assert!(batch.batch.len() < 5);
    assert!(!queue.is_empty());
    assert_eq!(batch.batch.len() + queue.len(), 5);
}

#[test]
fn batch_single_oversized_message() {
    let mut queue = VecDeque::new();
    queue.push_back(make_proto_unit_with_shard(vec![42; 1000]));

    // The first message is always taken even if oversized
    let batch = Handler::create_message_batch(&mut queue, 10);
    assert_eq!(batch.batch.len(), 1);
    assert!(queue.is_empty());
}
