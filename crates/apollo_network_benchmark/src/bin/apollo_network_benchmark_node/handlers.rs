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
    BROADCAST_MESSAGE_HEARTBEAT_MILLIS,
    BROADCAST_MESSAGE_SEND_DELAY_SECONDS,
    BROADCAST_MESSAGE_THROUGHPUT,
    RECEIVE_MESSAGE_BYTES,
    RECEIVE_MESSAGE_BYTES_SUM,
    RECEIVE_MESSAGE_COUNT,
    RECEIVE_MESSAGE_DELAY_SECONDS,
    RECEIVE_MESSAGE_PENDING_COUNT,
};
use crate::protocol::MessageSender;

/// `(sender_id, message_index)` forwarded from the network receive task to the
/// message-index tracker task.
pub struct IndexedMessage {
    pub sender_id: u64,
    pub message_index: u64,
}

fn update_broadcast_metrics(message_size_bytes: usize, heartbeat_interval: Duration) {
    BROADCAST_MESSAGE_HEARTBEAT_MILLIS.set(heartbeat_interval.as_millis().into_f64());
    BROADCAST_MESSAGE_THROUGHPUT.set(get_throughput(message_size_bytes, heartbeat_interval));
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
fn should_broadcast_round_robin(args: &NodeArgs) -> bool {
    let round_duration_seconds = args.user.round_duration_seconds;
    if round_duration_seconds == 0 {
        // A zero-duration round has no meaningful current owner.
        return false;
    }
    // `bootstrap` is the list of *other* peers, so total node count includes self.
    let num_nodes = u64::try_from(args.runner.bootstrap.len().saturating_add(1))
        .expect("num_nodes fits in u64 on all supported platforms");
    let now_seconds = seconds_since_epoch();
    let current_round = (now_seconds / round_duration_seconds) % num_nodes;
    args.runner.id == current_round
}

/// Unified implementation for sending stress test messages via any protocol
pub async fn send_stress_test_messages(mut message_sender: MessageSender, args: &NodeArgs) {
    let message_size_bytes = args.user.message_size_bytes;
    let heartbeat_interval = Duration::from_millis(args.user.heartbeat_millis);

    let mut message_index = 0;
    let mut message = create_message(args.runner.id, message_size_bytes);

    let mut interval = tokio::time::interval(heartbeat_interval);
    loop {
        interval.tick().await;

        // Check if this node should broadcast based on the mode
        let should_broadcast_now = match args.user.mode {
            Mode::AllBroadcast | Mode::OneBroadcast => true,
            Mode::RoundRobin => should_broadcast_round_robin(args),
        };

        if should_broadcast_now {
            update_broadcast_metrics(message.len(), heartbeat_interval);
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
            BROADCAST_MESSAGE_THROUGHPUT.set(0.0);
        }
    }
}

/// Records the receipt of a single inbound stress-test message:
/// - Updates byte/count metrics for every received payload.
/// - Panics on clock skew (sender timestamp ahead of receiver) so the benchmark fails loudly rather
///   than reporting a misleading latency histogram.
/// - Drops malformed payloads with a `warn!` and returns without recording a sample.
/// - Drops index-tracker samples (with a `debug!`) when the tracker channel is full so a slow
///   tracker can't stall the network receive task or grow memory without bound.
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
    // Drop on full so a slow tracker task can't stall the network receive task or grow
    // memory without bound. The pending-count metric will lag rather than OOM. Use debug!
    // because under sustained backpressure this fires per message and would flood logs.
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
