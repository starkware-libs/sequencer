use std::time::{Duration, SystemTime};

use apollo_metrics::metrics::LossyIntoF64;
use apollo_network_benchmark::node_args::NodeArgs;
use libp2p::PeerId;

use crate::message::{StressTestMessage, METADATA_SIZE};
use crate::metrics::{
    get_throughput,
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

/// Unified implementation for sending stress test messages via any protocol
pub async fn send_stress_test_messages(
    mut message_sender: MessageSender,
    args: &NodeArgs,
    peers: Vec<PeerId>,
) {
    let size_bytes = args.user.message_size_bytes;
    let heartbeat = Duration::from_millis(args.user.heartbeat_millis);

    let mut message_index = 0;
    let mut message = get_message(args.runner.id, size_bytes).clone();
    update_broadcast_metrics(message.len(), heartbeat);

    let mut interval = tokio::time::interval(heartbeat);
    loop {
        interval.tick().await;

        message.metadata.time = SystemTime::now();
        message.metadata.message_index = message_index;
        let message_clone = message.clone().into();
        let start_time = std::time::Instant::now();
        message_sender.send_message(&peers, message_clone).await;
        BROADCAST_MESSAGE_SEND_DELAY_SECONDS.record(start_time.elapsed().as_secs_f64());
        BROADCAST_MESSAGE_BYTES.set(message.len().into_f64());
        BROADCAST_MESSAGE_COUNT.increment(1);
        BROADCAST_MESSAGE_BYTES_SUM
            .increment(u64::try_from(message.len()).expect("Message length too large for u64"));
        message_index += 1;
    }
}

pub fn receive_stress_test_message(received_message: Vec<u8>, _sender_peer_id: Option<PeerId>) {
    let end_time = SystemTime::now();

    let received_message: StressTestMessage = received_message.into();
    let start_time = received_message.metadata.time;
    let delay_seconds = end_time
        .duration_since(start_time)
        .expect("End time should be after start time (Probably clock misalignment)")
        .as_secs_f64();

    // Use apollo_metrics for all metrics including labeled ones
    RECEIVE_MESSAGE_BYTES.set(received_message.len().into_f64());
    RECEIVE_MESSAGE_COUNT.increment(1);
    RECEIVE_MESSAGE_BYTES_SUM.increment(
        u64::try_from(received_message.len()).expect("Message length too large for u64"),
    );

    // Use apollo_metrics histograms for latency measurements
    RECEIVE_MESSAGE_DELAY_SECONDS.record(delay_seconds);
}
