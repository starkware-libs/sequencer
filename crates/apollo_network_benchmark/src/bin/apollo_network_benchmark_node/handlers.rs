use std::time::{Duration, SystemTime};

use apollo_metrics::metrics::LossyIntoF64;
use apollo_network_benchmark::node_args::{Mode, NodeArgs};
use libp2p::PeerId;
use tracing::{trace, warn};

use crate::message::{StressTestMessage, METADATA_SIZE};
use crate::message_index_detector::MessageIndexTracker;
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

fn update_broadcast_metrics(message_size_bytes: usize, broadcast_heartbeat: Duration) {
    BROADCAST_MESSAGE_HEARTBEAT_MILLIS.set(broadcast_heartbeat.as_millis().into_f64());
    BROADCAST_MESSAGE_THROUGHPUT.set(get_throughput(message_size_bytes, broadcast_heartbeat));
}

fn create_message(id: u64, size_bytes: usize) -> StressTestMessage {
    let message = StressTestMessage::new(id, 0, vec![0; size_bytes.saturating_sub(*METADATA_SIZE)]);
    debug_assert_eq!(Vec::<u8>::from(message.clone()).len(), size_bytes);
    message
}

// TODO(AndrewL): Round ownership is derived from each node's local wall clock, so clock skew
// between nodes can produce overlap/gaps at round boundaries and pollute RR-mode results.
// Requires NTP-synced nodes; ideally switch to a coordinated round source.
fn should_broadcast_round_robin(args: &NodeArgs) -> bool {
    let now_seconds = seconds_since_epoch();
    let round_duration_seconds = args.user.round_duration_seconds;
    let num_bootstrap_peers = u64::try_from(args.runner.bootstrap.len())
        .expect("bootstrap list length fits in u64 on all supported platforms");
    let current_round = (now_seconds / round_duration_seconds) % num_bootstrap_peers;
    args.runner.id == current_round
}

/// Unified implementation for sending stress test messages via any protocol
pub async fn send_stress_test_messages(mut message_sender: MessageSender, args: &NodeArgs) {
    let size_bytes = args.user.message_size_bytes;
    let heartbeat = Duration::from_millis(args.user.heartbeat_millis);

    let mut message_index = 0;
    let mut message = create_message(args.runner.id, size_bytes);

    let mut interval = tokio::time::interval(heartbeat);
    loop {
        interval.tick().await;

        // Check if this node should broadcast based on the mode
        let should_broadcast_now = match args.user.mode {
            Mode::AllBroadcast | Mode::OneBroadcast => true,
            Mode::RoundRobin => should_broadcast_round_robin(args),
        };

        if should_broadcast_now {
            update_broadcast_metrics(message.len(), heartbeat);
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

pub fn receive_stress_test_message(
    received_bytes: Vec<u8>,
    _sender_peer_id: Option<PeerId>,
    tx: tokio::sync::mpsc::UnboundedSender<(usize, u64)>,
) {
    let end_time = SystemTime::now();

    let received_message = match StressTestMessage::try_from(received_bytes) {
        Ok(message) => message,
        Err(parse_error) => {
            warn!("Dropping malformed message: {parse_error}");
            return;
        }
    };
    let start_time = received_message.metadata.time;
    // Skew or out-of-order clocks can put the sender's timestamp in our future;
    // record 0 delay in that case instead of killing the receiver task.
    let delay_seconds = match end_time.duration_since(start_time) {
        Ok(delay) => delay.as_secs_f64(),
        Err(skew) => {
            warn!(
                "Dropping delay sample: sender timestamp is ahead of receiver by {:?}",
                skew.duration()
            );
            0.0
        }
    };

    RECEIVE_MESSAGE_BYTES.set(received_message.len().into_f64());
    RECEIVE_MESSAGE_COUNT.increment(1);
    RECEIVE_MESSAGE_BYTES_SUM.increment(
        u64::try_from(received_message.len())
            .expect("usize fits in u64 on all supported platforms"),
    );

    RECEIVE_MESSAGE_DELAY_SECONDS.record(delay_seconds);

    let Ok(sender_id) = usize::try_from(received_message.metadata.sender_id) else {
        warn!("Dropping message: sender_id {} exceeds usize", received_message.metadata.sender_id);
        return;
    };
    // Unbounded channel send only fails if the receiver is closed, which means the
    // tracker task has ended; the process is about to exit anyway.
    let _ = tx.send((sender_id, received_message.metadata.message_index));
}

pub async fn record_indexed_message(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<(usize, u64)>,
    num_peers: usize,
) {
    let mut index_tracker = vec![MessageIndexTracker::default(); num_peers];
    let mut all_pending = 0;
    while let Some((peer_id, index)) = rx.recv().await {
        let old_pending = index_tracker[peer_id].pending_messages_count();
        index_tracker[peer_id].seen_message(index);
        let new_pending = index_tracker[peer_id].pending_messages_count();

        all_pending -= old_pending;
        all_pending += new_pending;

        RECEIVE_MESSAGE_PENDING_COUNT.set(all_pending.into_f64());
    }
}
