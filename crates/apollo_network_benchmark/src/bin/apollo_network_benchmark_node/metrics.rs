use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use apollo_metrics::metrics::LossyIntoF64;
use apollo_network::metrics::{
    BroadcastNetworkMetrics,
    EventMetrics,
    LatencyMetrics,
    NetworkMetrics,
    SqmrNetworkMetrics,
};
pub use apollo_network_benchmark::metrics::*;

use crate::protocol::TOPIC;

pub(crate) fn register_metrics() {
    BROADCAST_MESSAGE_HEARTBEAT_MILLIS.register();
    BROADCAST_MESSAGE_THROUGHPUT.register();
    BROADCAST_MESSAGE_BYTES.register();
    BROADCAST_MESSAGE_COUNT.register();
    BROADCAST_MESSAGE_BYTES_SUM.register();
    BROADCAST_MESSAGE_SEND_DELAY_SECONDS.register();
    RECEIVE_MESSAGE_BYTES.register();
    RECEIVE_MESSAGE_COUNT.register();
    RECEIVE_MESSAGE_BYTES_SUM.register();
    RECEIVE_MESSAGE_DELAY_SECONDS.register();
}

pub fn get_throughput(message_size_bytes: usize, heartbeat_duration: Duration) -> f64 {
    let tps = Duration::from_secs(1).as_secs_f64() / heartbeat_duration.as_secs_f64();
    tps * message_size_bytes.into_f64()
}

pub fn seconds_since_epoch() -> u64 {
    let now = SystemTime::now();
    now.duration_since(UNIX_EPOCH).unwrap().as_secs()
}

pub fn create_network_metrics() -> apollo_network::metrics::NetworkMetrics {
    let stress_test_broadcast_metrics = BroadcastNetworkMetrics {
        sent_broadcast_message_metrics: apollo_network::metrics::MessageMetrics {
            num_messages: NETWORK_STRESS_TEST_SENT_MESSAGES,
            message_size_mb: None,
        },
        dropped_broadcast_message_metrics: apollo_network::metrics::LabeledMessageMetrics {
            num_messages: NETWORK_DROPPED_BROADCAST_MESSAGES,
            message_size_mb: None,
        },
        received_broadcast_message_metrics: apollo_network::metrics::MessageMetrics {
            num_messages: NETWORK_STRESS_TEST_RECEIVED_MESSAGES,
            message_size_mb: None,
        },
    };

    let mut broadcast_metrics_by_topic = HashMap::new();
    broadcast_metrics_by_topic.insert(TOPIC.hash(), stress_test_broadcast_metrics);

    let sqmr_metrics = SqmrNetworkMetrics {
        num_active_inbound_sessions: NETWORK_ACTIVE_INBOUND_SESSIONS,
        num_active_outbound_sessions: NETWORK_ACTIVE_OUTBOUND_SESSIONS,
    };

    let event_metrics = EventMetrics { event_counter: NETWORK_EVENT_COUNTER };

    let latency_metrics = LatencyMetrics { ping_latency_seconds: PING_LATENCY_SECONDS };

    NetworkMetrics {
        num_connected_peers: NETWORK_CONNECTED_PEERS,
        num_blacklisted_peers: NETWORK_BLACKLISTED_PEERS,
        broadcast_metrics_by_topic: Some(broadcast_metrics_by_topic),
        sqmr_metrics: Some(sqmr_metrics),
        event_metrics: Some(event_metrics),
        latency_metrics: Some(latency_metrics),
    }
}
