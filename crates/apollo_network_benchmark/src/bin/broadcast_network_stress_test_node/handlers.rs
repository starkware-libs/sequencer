use std::time::SystemTime;

use libp2p::PeerId;
use tracing::trace;

use crate::message::StressTestMessage;

pub fn receive_stress_test_message(received_message: Vec<u8>, sender_peer_id: Option<PeerId>) {
    let end_time = SystemTime::now();

    let received_message: StressTestMessage = received_message.into();
    let start_time = received_message.metadata.time;
    let delay_seconds = end_time
        .duration_since(start_time)
        .expect("End time should be after start time (Probably clock misalignment)")
        .as_secs_f64();

    // TODO(AndrewL): Replace this with metric updates
    trace!("Received stress test message from {sender_peer_id:?} in {delay_seconds} seconds");
}
