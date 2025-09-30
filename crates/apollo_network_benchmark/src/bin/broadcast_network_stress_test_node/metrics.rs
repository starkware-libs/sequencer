use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
use apollo_propeller::metrics::{
    PropellerMetrics,
    COLLECTION_LENGTH_LABELS,
    SHARD_SEND_FAILURE_LABELS,
    SHARD_VALIDATION_FAILURE_LABELS,
};

use crate::protocol::TOPIC;

define_metrics!(
    Infra => {
        MetricGauge { BROADCAST_MESSAGE_HEARTBEAT_MILLIS, "broadcast_message_theoretical_heartbeat_millis", "The number of ms we sleep between each two consecutive broadcasts" },
        MetricGauge { BROADCAST_MESSAGE_THROUGHPUT, "broadcast_message_theoretical_throughput", "Throughput in bytes/second of the broadcasted " },
        MetricGauge { BROADCAST_MESSAGE_BYTES, "broadcast_message_bytes", "Size of the stress test sent message in bytes" },
        MetricCounter { BROADCAST_MESSAGE_COUNT, "broadcast_message_count", "Number of stress test messages sent via broadcast", init = 0 },
        MetricCounter { BROADCAST_MESSAGE_BYTES_SUM, "broadcast_message_bytes_sum", "Sum of the stress test messages sent via broadcast", init = 0 },
        MetricHistogram { BROADCAST_MESSAGE_SEND_DELAY_SECONDS, "broadcast_message_send_delay_seconds", "Message sending delay in seconds" },

        MetricGauge { RECEIVE_MESSAGE_BYTES, "receive_message_bytes", "Size of the stress test received message in bytes" },
        MetricGauge { RECEIVE_MESSAGE_PENDING_COUNT, "receive_message_pending_count", "Number of stress test messages pending to be received" },
        MetricCounter { RECEIVE_MESSAGE_COUNT, "receive_message_count", "Number of stress test messages received via broadcast", init = 0 },
        MetricCounter { RECEIVE_MESSAGE_BYTES_SUM, "receive_message_bytes_sum", "Sum of the stress test messages received via broadcast", init = 0 },
        MetricHistogram { RECEIVE_MESSAGE_DELAY_SECONDS, "receive_message_delay_seconds", "Message delay in seconds" },
        MetricHistogram { RECEIVE_MESSAGE_NEGATIVE_DELAY_SECONDS, "receive_message_negative_delay_seconds", "Negative message delay in seconds" },

        MetricGauge { NETWORK_CONNECTED_PEERS, "network_connected_peers", "Number of connected peers in the network" },
        MetricGauge { NETWORK_BLACKLISTED_PEERS, "network_blacklisted_peers", "Number of blacklisted peers in the network" },
        MetricGauge { NETWORK_ACTIVE_INBOUND_SESSIONS, "network_active_inbound_sessions", "Number of active inbound SQMR sessions" },
        MetricGauge { NETWORK_ACTIVE_OUTBOUND_SESSIONS, "network_active_outbound_sessions", "Number of active outbound SQMR sessions" },
        MetricCounter { NETWORK_STRESS_TEST_SENT_MESSAGES, "network_stress_test_sent_messages", "Number of stress test messages sent via broadcast", init = 0 },
        MetricCounter { NETWORK_STRESS_TEST_RECEIVED_MESSAGES, "network_stress_test_received_messages", "Number of stress test messages received via broadcast", init = 0 },

        MetricGauge { SYSTEM_PROCESS_CPU_USAGE_PERCENT, "system_process_cpu_usage_percent", "CPU usage percentage of the current process" },
        MetricGauge { SYSTEM_PROCESS_MEMORY_USAGE_BYTES, "system_process_memory_usage_bytes", "Memory usage in bytes of the current process" },
        MetricGauge { SYSTEM_PROCESS_VIRTUAL_MEMORY_USAGE_BYTES, "system_process_virtual_memory_usage_bytes", "Virtual memory usage in bytes of the current process" },
        MetricGauge { SYSTEM_NETWORK_BYTES_SENT_TOTAL, "system_network_bytes_sent_total", "Total bytes sent across all network interfaces since system start" },
        MetricGauge { SYSTEM_NETWORK_BYTES_RECEIVED_TOTAL, "system_network_bytes_received_total", "Total bytes received across all network interfaces since system start" },
        MetricGauge { SYSTEM_NETWORK_BYTES_SENT_CURRENT, "system_network_bytes_sent_current", "Bytes sent across all network interfaces since last measurement" },
        MetricGauge { SYSTEM_NETWORK_BYTES_RECEIVED_CURRENT, "system_network_bytes_received_current", "Bytes received across all network interfaces since last measurement" },
        MetricGauge { SYSTEM_TOTAL_MEMORY_BYTES, "system_total_memory_bytes", "Total system memory in bytes" },
        MetricGauge { SYSTEM_AVAILABLE_MEMORY_BYTES, "system_available_memory_bytes", "Available system memory in bytes" },
        MetricGauge { SYSTEM_USED_MEMORY_BYTES, "system_used_memory_bytes", "Used system memory in bytes" },
        MetricGauge { SYSTEM_CPU_COUNT, "system_cpu_count", "Number of logical CPU cores in the system" },
        MetricGauge { SYSTEM_TCP_RETRANSMIT_RATE_PERCENT, "system_tcp_retransmit_rate_percent", "TCP retransmission rate as a percentage of segments sent (proxy for packet loss)" },
        MetricGauge { SYSTEM_TCP_SEGMENTS_OUT, "system_tcp_segments_out", "Total TCP segments sent by the system" },
        MetricGauge { SYSTEM_TCP_SEGMENTS_RETRANS, "system_tcp_segments_retrans", "Total TCP segments retransmitted by the system" },

        MetricCounter { NETWORK_RESET_TOTAL, "network_reset_total", "Total number of network resets performed", init = 0 },
        LabeledMetricCounter { NETWORK_DROPPED_BROADCAST_MESSAGES, "network_dropped_broadcast_messages", "Number of dropped broadcast messages by reason", init = 0, labels = NETWORK_BROADCAST_DROP_LABELS },
        LabeledMetricCounter { NETWORK_EVENT_COUNTER, "network_event_counter", "Network events counter by type", init = 0, labels = EVENT_TYPE_LABELS },

        MetricHistogram { PING_LATENCY_SECONDS, "ping_latency_seconds", "Ping latency in seconds" },

        // Propeller Protocol Metrics
        MetricCounter { PROPELLER_SHARDS_PUBLISHED, "propeller_shards_published", "Total number of shards published (created) by this node", init = 0 },
        MetricCounter { PROPELLER_SHARDS_SENT, "propeller_shards_sent", "Total number of shards sent to peers (includes forwarding)", init = 0 },
        LabeledMetricCounter { PROPELLER_SHARDS_SEND_FAILED, "propeller_shards_send_failed", "Total number of shard send failures, labeled by reason", init = 0, labels = SHARD_SEND_FAILURE_LABELS },
        MetricCounter { PROPELLER_SHARD_BYTES_SENT, "propeller_shard_bytes_sent", "Total bytes sent in shard data (payload only)", init = 0 },
        MetricCounter { PROPELLER_SHARDS_RECEIVED, "propeller_shards_received", "Total number of shards received from peers", init = 0 },
        LabeledMetricCounter { PROPELLER_SHARDS_VALIDATION_FAILED, "propeller_shards_validation_failed", "Total number of shards that failed validation, labeled by reason", init = 0, labels = SHARD_VALIDATION_FAILURE_LABELS },
        MetricCounter { PROPELLER_SHARDS_FORWARDED, "propeller_shards_forwarded", "Total number of shards forwarded to children in tree", init = 0 },
        MetricCounter { PROPELLER_SHARD_BYTES_RECEIVED, "propeller_shard_bytes_received", "Total bytes received in shard data (payload only)", init = 0 },
        MetricCounter { PROPELLER_MESSAGES_RECONSTRUCTED, "propeller_messages_reconstructed", "Total number of messages successfully reconstructed from shards", init = 0 },
        MetricCounter { PROPELLER_MESSAGES_RECONSTRUCTION_FAILED, "propeller_messages_reconstruction_failed", "Total number of message reconstruction failures", init = 0 },
        MetricCounter { PROPELLER_TREES_GENERATED, "propeller_trees_generated", "Total number of tree generation operations", init = 0 },
        LabeledMetricGauge { PROPELLER_COLLECTION_LENGTHS, "propeller_collection_lengths", "Length of various collections (queues, sets, caches) tracked by label", labels = COLLECTION_LENGTH_LABELS },
    },
);

pub fn seconds_since_epoch() -> u64 {
    let now = SystemTime::now();
    now.duration_since(UNIX_EPOCH).unwrap().as_secs()
}

/// Calculates the throughput given the message and how much to sleep between each two consecutive
/// broadcasts
pub fn get_throughput(message_size_bytes: usize, heartbeat_duration: Duration) -> f64 {
    let tps = Duration::from_secs(1).as_secs_f64() / heartbeat_duration.as_secs_f64();
    tps * message_size_bytes.into_f64()
}

/// Creates comprehensive network metrics for monitoring the stress test network performance.
/// Uses the lazy static metrics defined above.
pub fn create_network_metrics() -> NetworkMetrics {
    // Create broadcast metrics for the stress test topic
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

    // Create a map with broadcast metrics for our stress test topic
    let mut broadcast_metrics_by_topic = HashMap::new();
    broadcast_metrics_by_topic.insert(TOPIC.hash(), stress_test_broadcast_metrics);

    // Create SQMR metrics for session monitoring
    let sqmr_metrics = SqmrNetworkMetrics {
        num_active_inbound_sessions: NETWORK_ACTIVE_INBOUND_SESSIONS,
        num_active_outbound_sessions: NETWORK_ACTIVE_OUTBOUND_SESSIONS,
    };

    // Create event metrics for network events monitoring
    let event_metrics = EventMetrics { event_counter: NETWORK_EVENT_COUNTER };

    // Create latency metrics for ping monitoring
    let latency_metrics = LatencyMetrics { ping_latency_seconds: PING_LATENCY_SECONDS };

    let propeller_metrics = PropellerMetrics {
        shards_published: PROPELLER_SHARDS_PUBLISHED,
        shards_sent: PROPELLER_SHARDS_SENT,
        shards_send_failed: PROPELLER_SHARDS_SEND_FAILED,
        shard_bytes_sent: PROPELLER_SHARD_BYTES_SENT,
        shards_received: PROPELLER_SHARDS_RECEIVED,
        shards_validation_failed: PROPELLER_SHARDS_VALIDATION_FAILED,
        shards_forwarded: PROPELLER_SHARDS_FORWARDED,
        shard_bytes_received: PROPELLER_SHARD_BYTES_RECEIVED,
        messages_reconstructed: PROPELLER_MESSAGES_RECONSTRUCTED,
        messages_reconstruction_failed: PROPELLER_MESSAGES_RECONSTRUCTION_FAILED,
        trees_generated: PROPELLER_TREES_GENERATED,
        collection_lengths: PROPELLER_COLLECTION_LENGTHS,
    };

    NetworkMetrics {
        num_connected_peers: NETWORK_CONNECTED_PEERS,
        num_blacklisted_peers: NETWORK_BLACKLISTED_PEERS,
        broadcast_metrics_by_topic: Some(broadcast_metrics_by_topic),
        sqmr_metrics: Some(sqmr_metrics),
        event_metrics: Some(event_metrics),
        latency_metrics: Some(latency_metrics),
        propeller_metrics: Some(propeller_metrics),
    }
}
