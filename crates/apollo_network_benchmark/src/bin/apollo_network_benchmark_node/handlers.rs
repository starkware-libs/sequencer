use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use apollo_metrics::metrics::LossyIntoF64;
use apollo_network_benchmark::node_args::{Mode, NodeArgs};
use libp2p::PeerId;
use tokio::sync::mpsc::{Receiver, Sender};
use tracing::{debug, trace, warn};

use crate::message::{StressTestMessage, METADATA_SIZE};
use crate::message_index_tracker::MessageIndexTracker;
use crate::metrics::{
    get_throughput,
    seconds_since_epoch,
    BROADCAST_MESSAGE_BYTES,
    BROADCAST_MESSAGE_BYTES_SUM,
    BROADCAST_MESSAGE_COUNT,
    BROADCAST_MESSAGE_SEND_DELAY_SECONDS,
    BROADCAST_MESSAGE_THEORETICAL_HEARTBEAT_MILLIS,
    BROADCAST_MESSAGE_THEORETICAL_THROUGHPUT,
    RECEIVE_MESSAGE_BYTES,
    RECEIVE_MESSAGE_BYTES_SUM,
    RECEIVE_MESSAGE_COUNT,
    RECEIVE_MESSAGE_DELAY_SECONDS,
    RECEIVE_MESSAGE_PENDING_COUNT,
};
use crate::protocol::MessageSender;

pub struct IndexedMessage {
    pub sender_id: u64,
    pub message_index: u64,
}

fn create_message(sender_id: u64, message_size_bytes: usize) -> StressTestMessage {
    let message = StressTestMessage::new(
        sender_id,
        0,
        vec![0; message_size_bytes.saturating_sub(*METADATA_SIZE)],
    );
    debug_assert_eq!(Vec::<u8>::from(message.clone()).len(), message_size_bytes);
    message
}

// TODO(AndrewL): Round ownership is derived from each node's local wall clock, so clock skew
// between nodes can produce overlap/gaps at round boundaries and pollute RR-mode results.
// Requires NTP-synced nodes; ideally switch to a coordinated round source.
//
// `--bootstrap` must list every *other* peer (no self), so `bootstrap.len() + 1` equals the
// total node count. A partial bootstrap list silently breaks round-robin.
fn round_robin_owner_at(now_seconds: u64, args: &NodeArgs) -> Option<u64> {
    let round_duration_seconds = args.user.round_duration_seconds;
    if round_duration_seconds == 0 {
        return None;
    }
    let num_nodes = u64::try_from(args.runner.bootstrap.len().saturating_add(1))
        .expect("num_nodes fits in u64 on all supported platforms");
    Some((now_seconds / round_duration_seconds) % num_nodes)
}

fn should_broadcast_round_robin(args: &NodeArgs) -> bool {
    round_robin_owner_at(seconds_since_epoch(), args) == Some(args.runner.id)
}

pub async fn send_stress_test_messages(mut message_sender: MessageSender, args: &NodeArgs) {
    let message_size_bytes = args.user.message_size_bytes;
    let heartbeat_interval = Duration::from_millis(args.user.heartbeat_millis);

    let mut message_index = 0;
    let mut message = create_message(args.runner.id, message_size_bytes);

    let mut interval = tokio::time::interval(heartbeat_interval);
    loop {
        interval.tick().await;

        let should_broadcast_now = match args.user.mode {
            Mode::AllBroadcast | Mode::OneBroadcast => true,
            Mode::RoundRobin => should_broadcast_round_robin(args),
        };

        if should_broadcast_now {
            BROADCAST_MESSAGE_THEORETICAL_HEARTBEAT_MILLIS
                .set(heartbeat_interval.as_millis().into_f64());
            BROADCAST_MESSAGE_THEORETICAL_THROUGHPUT
                .set(get_throughput(message.len(), heartbeat_interval));
            message.metadata.time = SystemTime::now();
            message.metadata.message_index = message_index;
            let message_clone = message.clone().into();
            let start_time = std::time::Instant::now();
            message_sender.send_message(message_clone).await;
            BROADCAST_MESSAGE_SEND_DELAY_SECONDS.record(start_time.elapsed().as_secs_f64());
            BROADCAST_MESSAGE_BYTES.set(message.len().into_f64());
            BROADCAST_MESSAGE_COUNT.increment(1);
            BROADCAST_MESSAGE_BYTES_SUM.increment(
                u64::try_from(message.len()).expect("usize fits in u64 on all supported platforms"),
            );
            trace!(
                "Node {} sent message {message_index} in mode `{}`",
                args.runner.id,
                args.user.mode
            );
            message_index += 1;
        } else {
            BROADCAST_MESSAGE_THEORETICAL_HEARTBEAT_MILLIS.set(0.0);
            BROADCAST_MESSAGE_THEORETICAL_THROUGHPUT.set(0.0);
        }
    }
}

pub fn receive_stress_test_message(
    received_bytes: Vec<u8>,
    _sender_peer_id: Option<PeerId>,
    indexed_message_sender: Sender<IndexedMessage>,
) {
    let end_time = SystemTime::now();

    let received_message = match StressTestMessage::try_from(received_bytes) {
        Ok(message) => message,
        Err(parse_error) => {
            warn!("Dropping malformed message: {parse_error}");
            return;
        }
    };

    RECEIVE_MESSAGE_BYTES.set(received_message.len().into_f64());
    RECEIVE_MESSAGE_COUNT.increment(1);
    RECEIVE_MESSAGE_BYTES_SUM.increment(
        u64::try_from(received_message.len())
            .expect("usize fits in u64 on all supported platforms"),
    );

    let start_time = received_message.metadata.time;
    // Intentionally panic on clock skew: any node clock running ahead of another node's
    // clock invalidates the latency histogram, and we'd rather fail the benchmark loudly
    // than silently report misleading percentiles. Operators must NTP-sync nodes.
    let delay =
        end_time.duration_since(start_time).expect("clock skew detected: sender clock is ahead");
    RECEIVE_MESSAGE_DELAY_SECONDS.record(delay.as_secs_f64());

    let indexed_message = IndexedMessage {
        sender_id: received_message.metadata.sender_id,
        message_index: received_message.metadata.message_index,
    };
    // Drop-on-full so a slow tracker can't stall the receive task or grow memory without
    // bound; debug! because sustained backpressure would otherwise flood logs.
    if let Err(send_error) = indexed_message_sender.try_send(indexed_message) {
        debug!("Dropping indexed-message sample: {send_error}");
    }
}

pub async fn record_indexed_messages(mut indexed_message_receiver: Receiver<IndexedMessage>) {
    let mut index_tracker_by_sender: HashMap<u64, MessageIndexTracker> = HashMap::new();
    let mut all_pending: u64 = 0;
    while let Some(indexed_message) = indexed_message_receiver.recv().await {
        let tracker = index_tracker_by_sender.entry(indexed_message.sender_id).or_default();
        let old_pending = tracker.pending_messages_count();
        tracker.seen_message(indexed_message.message_index);
        let new_pending = tracker.pending_messages_count();

        all_pending = all_pending.saturating_sub(old_pending).saturating_add(new_pending);

        RECEIVE_MESSAGE_PENDING_COUNT.set(all_pending.into_f64());
    }
}

#[cfg(test)]
mod tests {
    use apollo_network_benchmark::node_args::{
        Mode,
        NetworkProtocol,
        NodeArgs,
        RunnerArgs,
        UserArgs,
    };

    use super::round_robin_owner_at;

    fn make_args(id: u64, num_other_peers: usize, round_duration_seconds: u64) -> NodeArgs {
        NodeArgs {
            runner: RunnerArgs {
                id,
                metric_port: 0,
                p2p_port: 0,
                bootstrap: vec![String::new(); num_other_peers],
            },
            user: UserArgs {
                verbosity: 0,
                buffer_size: 0,
                mode: Mode::RoundRobin,
                network_protocol: NetworkProtocol::Gossipsub,
                broadcaster: None,
                round_duration_seconds,
                message_size_bytes: 0,
                heartbeat_millis: 1,
                timeout_seconds: 0,
            },
        }
    }

    #[test]
    fn zero_round_duration_yields_no_owner() {
        let args = make_args(0, 2, 0);
        assert_eq!(round_robin_owner_at(0, &args), None);
        assert_eq!(round_robin_owner_at(1_234_567, &args), None);
    }

    #[test]
    fn single_node_always_owns_the_round() {
        // bootstrap is empty → num_nodes = 1; owner is always node 0.
        let args = make_args(0, 0, 3);
        for now_seconds in [0u64, 1, 2, 3, 100, 100_000] {
            assert_eq!(round_robin_owner_at(now_seconds, &args), Some(0));
        }
    }

    #[test]
    fn ownership_rotates_across_nodes_at_round_boundaries() {
        // Three nodes, 5-second rounds. The current node id is unused by `round_robin_owner_at`,
        // so we just check the schedule.
        let args = make_args(0, 2, 5);
        assert_eq!(round_robin_owner_at(0, &args), Some(0));
        assert_eq!(round_robin_owner_at(4, &args), Some(0));
        assert_eq!(round_robin_owner_at(5, &args), Some(1));
        assert_eq!(round_robin_owner_at(9, &args), Some(1));
        assert_eq!(round_robin_owner_at(10, &args), Some(2));
        assert_eq!(round_robin_owner_at(14, &args), Some(2));
        // Wraps back to node 0 on the next cycle.
        assert_eq!(round_robin_owner_at(15, &args), Some(0));
    }
}
