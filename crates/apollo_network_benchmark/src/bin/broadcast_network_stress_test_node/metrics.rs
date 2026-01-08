use std::collections::HashMap;
use std::time::Duration;

use apollo_metrics::define_metrics;
use apollo_metrics::metrics::LossyIntoF64;
use apollo_network::metrics::{
    BroadcastNetworkMetrics,
    EventMetrics,
    LatencyMetrics,
    NetworkMetrics,
    SqmrNetworkMetrics,
    EVENT_TYPE_LABELS,
    NETWORK_BROADCAST_DROP_LABELS,
};

use crate::protocol::TOPIC;

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
        MetricGauge { RECEIVE_MESSAGE_PENDING_COUNT, "receive_message_pending_count", "Number of stress test messages pending to be received" },

        MetricGauge { NETWORK_CONNECTED_PEERS, "network_connected_peers", "Number of connected peers in the network" },
        MetricGauge { NETWORK_BLACKLISTED_PEERS, "network_blacklisted_peers", "Number of blacklisted peers in the network" },
        MetricGauge { NETWORK_ACTIVE_INBOUND_SESSIONS, "network_active_inbound_sessions", "Number of active inbound SQMR sessions" },
        MetricGauge { NETWORK_ACTIVE_OUTBOUND_SESSIONS, "network_active_outbound_sessions", "Number of active outbound SQMR sessions" },
        MetricCounter { NETWORK_STRESS_TEST_SENT_MESSAGES, "network_stress_test_sent_messages", "Number of stress test messages sent via broadcast", init = 0 },
        MetricCounter { NETWORK_STRESS_TEST_RECEIVED_MESSAGES, "network_stress_test_received_messages", "Number of stress test messages received via broadcast", init = 0 },
        LabeledMetricCounter { NETWORK_DROPPED_BROADCAST_MESSAGES, "network_dropped_broadcast_messages", "Number of dropped broadcast messages by reason", init = 0, labels = NETWORK_BROADCAST_DROP_LABELS },
        LabeledMetricCounter { NETWORK_EVENT_COUNTER, "network_event_counter", "Network events counter by type", init = 0, labels = EVENT_TYPE_LABELS },

        MetricHistogram { PING_LATENCY_SECONDS, "ping_latency_seconds", "Ping latency in seconds" },
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
