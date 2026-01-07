use std::time::Duration;

use apollo_metrics::define_metrics;
use apollo_metrics::metrics::LossyIntoF64;
use apollo_network::metrics::NetworkMetrics;

define_metrics!(
    Infra => {
        MetricGauge { BROADCAST_MESSAGE_HEARTBEAT_MILLIS, "broadcast_message_theoretical_heartbeat_millis", "The number of ms we sleep between each two consecutive broadcasts" },
        MetricGauge { BROADCAST_MESSAGE_THROUGHPUT, "broadcast_message_theoretical_throughput", "Throughput in bytes/second of the broadcasted messages" },
        MetricGauge { BROADCAST_MESSAGE_BYTES, "broadcast_message_bytes", "Size of the stress test sent message in bytes" },
        MetricCounter { BROADCAST_MESSAGE_COUNT, "broadcast_message_count", "Number of stress test messages sent via broadcast", init = 0 },
        MetricCounter { BROADCAST_MESSAGE_BYTES_SUM, "broadcast_message_bytes_sum", "Sum of the stress test messages sent via broadcast", init = 0 },
        MetricHistogram { BROADCAST_MESSAGE_SEND_DELAY_SECONDS, "broadcast_message_send_delay_seconds", "Message sending delay in seconds" },

        MetricGauge { RECEIVE_MESSAGE_BYTES, "receive_message_bytes", "Size of the stress test received message in bytes" },
        MetricCounter { RECEIVE_MESSAGE_COUNT, "receive_message_count", "Number of stress test messages received via broadcast", init = 0 },
        MetricCounter { RECEIVE_MESSAGE_BYTES_SUM, "receive_message_bytes_sum", "Sum of the stress test messages received via broadcast", init = 0 },
        MetricHistogram { RECEIVE_MESSAGE_DELAY_SECONDS, "receive_message_delay_seconds", "Message delay in seconds" },

        MetricGauge { NETWORK_CONNECTED_PEERS, "network_connected_peers", "Number of connected peers in the network" },
        MetricGauge { NETWORK_BLACKLISTED_PEERS, "network_blacklisted_peers", "Number of blacklisted peers in the network" },
    },
);

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

/// Calculates the throughput given the message size and heartbeat duration
pub fn get_throughput(message_size_bytes: usize, heartbeat_duration: Duration) -> f64 {
    let tps = Duration::from_secs(1).as_secs_f64() / heartbeat_duration.as_secs_f64();
    tps * message_size_bytes.into_f64()
}

/// Creates barebones network metrics
pub fn create_network_metrics() -> apollo_network::metrics::NetworkMetrics {
    NetworkMetrics {
        num_connected_peers: NETWORK_CONNECTED_PEERS,
        num_blacklisted_peers: NETWORK_BLACKLISTED_PEERS,
        broadcast_metrics_by_topic: None,
        sqmr_metrics: None,
        event_metrics: None,
        latency_metrics: None,
    }
}
