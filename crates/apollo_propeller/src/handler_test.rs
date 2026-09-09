use std::collections::VecDeque;
use std::time::Duration;

use apollo_protobuf::protobuf::PropellerUnit as ProtoUnit;
use futures::StreamExt;
use libp2p::swarm::SwarmEvent;
use libp2p::Swarm;
use libp2p_swarm_test::SwarmExt as _;
use prost::encoding::encoded_len_varint;
use prost::Message;
use starknet_api::staking::StakingWeight;
use tracing_test::traced_test;

use super::{Handler, QUEUE_WARNING_THRESHOLD};
use crate::types::{CommitteeId, Event};
use crate::{Behaviour, Config};

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

/// Regression test for an inbound backlog bound bypass: `Handler::unsent_units` is documented
/// to hold "at most one batch worth of units", and `poll_inner` only starts a new read pass
/// while it is empty. But `poll_single_inbound_substream_waiting_input` kept decoding every
/// already-buffered batch in a single call regardless of that gate, so a peer whose batches are
/// all already sitting in the transport by the time it's first polled could have every one of
/// them decoded -- and buffered in `unsent_units` -- in one shot, well past the one-batch bound.
///
/// This lives here (a unit test, same crate as `Handler`) rather than in `tests/e2e_test.rs`
/// because `tracing-test`'s default per-crate log filter only captures logs from the crate under
/// test; an integration test in `tests/` is a separate crate and would silently see none of
/// `apollo_propeller`'s own log output (including the `warn_every_n_ms!` backlog warning this
/// test relies on).
///
/// The signal is `poll_inner`'s backlog warning, which fires once any of its queues exceeds
/// `QUEUE_WARNING_THRESHOLD`. In this scenario only `unsent_units` can reach that size, so the
/// warning is unambiguous: the sender's `send_queue` is fed over libp2p's bounded per-connection
/// command channel and drained on every poll, and `events_to_emit` stays empty while no send fails.
///
/// `MAX_WIRE_MESSAGE_SIZE` is load-bearing in both directions: small enough that `NUM_MESSAGES`
/// units span many wire batches (so an unbounded read pass has plenty to over-consume), yet large
/// enough that one batch holds far fewer than `QUEUE_WARNING_THRESHOLD` units -- otherwise a single
/// legitimate batch would trip the warning and fail the test even with the bound in place.
///
/// Every broadcast is queued before `sender`'s swarm is ever polled, so once `sender` starts
/// running, it drains its whole backlog and pushes every batch onto the wire before `receiver`
/// is polled even once -- exactly the condition the one-batch bound must survive. Requiring all
/// `NUM_MESSAGES` to arrive also covers the liveness half of the fix: dropping the self-wake in
/// `poll_single_inbound_substream_waiting_input` leaves the already-buffered data undrained and
/// delivery stalls partway, tripping the timeout below.
#[traced_test]
#[tokio::test(flavor = "current_thread")]
async fn poll_inner_bounds_inbound_backlog_to_one_batch() {
    // Enough units in flight that an unbounded read pass buffers well past
    // QUEUE_WARNING_THRESHOLD in a single pass. Derived from the threshold rather than hardcoded
    // so that raising the threshold cannot silently turn this into a test that always passes.
    const NUM_MESSAGES: usize = 3 * QUEUE_WARNING_THRESHOLD;
    const MAX_WIRE_MESSAGE_SIZE: usize = 2048;
    // Generous relative to the ~1s this takes locally: the timeout only has to distinguish
    // "delivery stalled" from "slow", so err towards a loaded CI machine rather than a flake.
    const DELIVERY_TIMEOUT: Duration = Duration::from_secs(30);

    let config = Config { max_wire_message_size: MAX_WIRE_MESSAGE_SIZE, ..Config::default() };
    let mut sender = Swarm::new_ephemeral_tokio(|keypair| Behaviour::new(keypair, config.clone()));
    let mut receiver =
        Swarm::new_ephemeral_tokio(|keypair| Behaviour::new(keypair, config.clone()));
    sender.listen().with_memory_addr_external().await;
    receiver.listen().with_memory_addr_external().await;
    sender.connect(&mut receiver).await;

    let committee_id = CommitteeId([0u8; 32]);
    let peers = vec![
        (*sender.local_peer_id(), StakingWeight(1)),
        (*receiver.local_peer_id(), StakingWeight(1)),
    ];
    for swarm in [&mut sender, &mut receiver] {
        swarm
            .behaviour_mut()
            .register_committee_peers(committee_id, peers.clone())
            .await
            .unwrap()
            .expect("Failed to register committee");
    }

    // Queue every broadcast before the sender's swarm is polled even once, so none of this
    // reaches the handler's send queue or the wire yet.
    for message_index in 0..NUM_MESSAGES {
        let message = vec![u8::try_from(message_index % 256).unwrap()];
        sender
            .behaviour_mut()
            .broadcast(committee_id, message)
            .await
            .unwrap()
            .expect("Broadcast should succeed");
    }

    // Drive the sender in the background. With nothing else runnable yet, it drains its entire
    // backlog -- queueing all NUM_MESSAGES units' worth of batches on the wire -- before
    // yielding, so they are all already buffered by the time `receiver` is polled below.
    tokio::spawn(sender.loop_on_next());

    let mut num_received = 0;
    let result = tokio::time::timeout(DELIVERY_TIMEOUT, async {
        while num_received < NUM_MESSAGES {
            if let SwarmEvent::Behaviour(Event::MessageReceived { .. }) =
                receiver.select_next_some().await
            {
                num_received += 1;
            }
        }
    })
    .await;
    assert!(result.is_ok(), "Timed out: received {num_received}/{NUM_MESSAGES} messages");

    assert!(
        !logs_contain("Backlog in propeller handler"),
        "receiver's unsent_units backlog exceeded the warning threshold, meaning far more than \
         one batch worth of units was buffered from a single already-queued burst"
    );
}
