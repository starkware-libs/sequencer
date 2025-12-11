use std::time::SystemTime;

use apollo_metrics::metrics::LossyIntoF64;
use libp2p::PeerId;

use crate::message::StressTestMessage;
use crate::metrics::{
    RECEIVE_MESSAGE_BYTES,
    RECEIVE_MESSAGE_BYTES_SUM,
    RECEIVE_MESSAGE_COUNT,
    RECEIVE_MESSAGE_DELAY_SECONDS,
    RECEIVE_MESSAGE_NEGATIVE_DELAY_SECONDS,
};

pub fn receive_stress_test_message(received_message: Vec<u8>, _sender_peer_id: Option<PeerId>) {
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
}
