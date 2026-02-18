use std::time::{Duration, SystemTime};

use apollo_metrics::metrics::LossyIntoF64;
use apollo_network_benchmark::node_args::{Mode, NodeArgs};
use libp2p::PeerId;
use tracing::trace;

use crate::explore_config::{ExploreConfiguration, ExplorePhase};
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
    RECEIVE_MESSAGE_NEGATIVE_DELAY_SECONDS,
    RECEIVE_MESSAGE_PENDING_COUNT,
};
use crate::protocol::MessageSender;

fn update_broadcast_metrics(message_size_bytes: usize, broadcast_heartbeat: Duration) {
    BROADCAST_MESSAGE_HEARTBEAT_MILLIS.set(broadcast_heartbeat.as_millis().into_f64());
    BROADCAST_MESSAGE_THROUGHPUT.set(get_throughput(message_size_bytes, broadcast_heartbeat));
}

fn get_message(id: u64, size_bytes: usize) -> StressTestMessage {
    let message = StressTestMessage::new(id, 0, vec![0; size_bytes - *METADATA_SIZE]);
    assert_eq!(Vec::<u8>::from(message.clone()).len(), size_bytes);
    message
}

fn should_broadcast_round_robin(args: &NodeArgs) -> bool {
    let now_seconds = seconds_since_epoch();
    let round_duration_seconds = args.user.round_duration_seconds;
    let num_nodes: u64 = args.runner.bootstrap.len().try_into().unwrap();
    let current_round = (now_seconds / round_duration_seconds) % num_nodes;
    args.runner.id == current_round
}

/// Unified implementation for sending stress test messages via any protocol
pub async fn send_stress_test_messages_impl(
    mut message_sender: MessageSender,
    args: &NodeArgs,
    peers: Vec<PeerId>,
    explore_config: &Option<ExploreConfiguration>,
) {
    let size_bytes = args.user.message_size_bytes;
    let heartbeat = Duration::from_millis(args.user.heartbeat_millis);

    let mut message_index = 0;
    let mut message = get_message(args.runner.id, size_bytes).clone();
    update_broadcast_metrics(message.len(), heartbeat);

    let mut interval = tokio::time::interval(heartbeat);
    loop {
        interval.tick().await;

        // Check if this node should broadcast based on the mode
        let should_broadcast_now = match args.user.mode {
            Mode::AllBroadcast | Mode::OneBroadcast => true,
            Mode::RoundRobin => should_broadcast_round_robin(args),
            Mode::Explore => {
                explore_config.as_ref().expect("ExploreConfig not available").get_current_phase()
                    == ExplorePhase::Running
            }
        };

        if should_broadcast_now {
            message.metadata.time = SystemTime::now();
            message.metadata.message_index = message_index;
            let message_clone = message.clone().into();
            let start_time = std::time::Instant::now();
            message_sender.send_message(&peers, message_clone).await;
            BROADCAST_MESSAGE_SEND_DELAY_SECONDS.record(start_time.elapsed().as_secs_f64());
            BROADCAST_MESSAGE_BYTES.set(message.len() as f64);
            BROADCAST_MESSAGE_COUNT.increment(1);
            BROADCAST_MESSAGE_BYTES_SUM.increment(message.len() as u64);
            trace!(
                "Node {} sent message {message_index} in mode `{}`",
                args.runner.id,
                args.user.mode
            );
            message_index += 1;
        }
    }
}

pub fn receive_stress_test_message(
    received_message: Vec<u8>,
    tx: tokio::sync::mpsc::UnboundedSender<(usize, u64)>,
) {
    let end_time = SystemTime::now();

    let received_message: StressTestMessage = received_message.into();
    let start_time = received_message.metadata.time;
    let delay_seconds = match end_time.duration_since(start_time) {
        Ok(duration) => duration.as_secs_f64(),
        Err(_) => {
            let negative_duration = start_time.duration_since(end_time).unwrap();
            -negative_duration.as_secs_f64()
        }
    };

    // Use apollo_metrics for all metrics including labeled ones
    RECEIVE_MESSAGE_BYTES.set(received_message.len().into_f64());
    RECEIVE_MESSAGE_COUNT.increment(1);
    RECEIVE_MESSAGE_BYTES_SUM.increment(
        u64::try_from(received_message.len()).expect("Message length too large for u64"),
    );

    // Use apollo_metrics histograms for latency measurements
    if delay_seconds.is_sign_positive() {
        RECEIVE_MESSAGE_DELAY_SECONDS.record(delay_seconds);
    } else {
        RECEIVE_MESSAGE_NEGATIVE_DELAY_SECONDS.record(-delay_seconds);
    }

    tx.send((
        received_message.metadata.sender_id as usize,
        received_message.metadata.message_index,
    ))
    .unwrap();
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
