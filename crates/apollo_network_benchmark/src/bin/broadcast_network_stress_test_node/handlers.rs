use std::time::SystemTime;

use libp2p::PeerId;
use tracing::trace;

use crate::message::StressTestMessage;

pub fn receive_stress_test_message(received_message: Vec<u8>, sender_peer_id: Option<PeerId>) {
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

    // TODO(AndrewL): Replace this with metric updates
    trace!("Received stress test message from {sender_peer_id:?} in {delay_seconds} seconds");
}
