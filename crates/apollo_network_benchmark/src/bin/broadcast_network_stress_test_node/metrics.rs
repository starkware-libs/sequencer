use std::time::Duration;

use apollo_metrics::define_metrics;
use apollo_metrics::metrics::LossyIntoF64;

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

        // system metrics for the node
        MetricGauge { SYSTEM_TOTAL_MEMORY_BYTES, "system_total_memory_bytes", "Total system memory in bytes" },
        MetricGauge { SYSTEM_AVAILABLE_MEMORY_BYTES, "system_available_memory_bytes", "Available system memory in bytes" },
        MetricGauge { SYSTEM_USED_MEMORY_BYTES, "system_used_memory_bytes", "Used system memory in bytes" },
        MetricGauge { SYSTEM_CPU_COUNT, "system_cpu_count", "Number of logical CPU cores in the system" },

        // system metrics for the process
        MetricGauge { SYSTEM_PROCESS_CPU_USAGE_PERCENT, "system_process_cpu_usage_percent", "CPU usage percentage of the current process" },
        MetricGauge { SYSTEM_PROCESS_MEMORY_USAGE_BYTES, "system_process_memory_usage_bytes", "Memory usage in bytes of the current process" },
        MetricGauge { SYSTEM_PROCESS_VIRTUAL_MEMORY_USAGE_BYTES, "system_process_virtual_memory_usage_bytes", "Virtual memory usage in bytes of the current process" },

        // system metrics for network usage
        MetricGauge { SYSTEM_NETWORK_BYTES_SENT_TOTAL, "system_network_bytes_sent_total", "Total bytes sent across all network interfaces since system start" },
        MetricGauge { SYSTEM_NETWORK_BYTES_RECEIVED_TOTAL, "system_network_bytes_received_total", "Total bytes received across all network interfaces since system start" },
        MetricGauge { SYSTEM_NETWORK_BYTES_SENT_CURRENT, "system_network_bytes_sent_current", "Bytes sent across all network interfaces since last measurement" },
        MetricGauge { SYSTEM_NETWORK_BYTES_RECEIVED_CURRENT, "system_network_bytes_received_current", "Bytes received across all network interfaces since last measurement" },
    },
);

/// Calculates the throughput given the message size and heartbeat duration
pub fn get_throughput(message_size_bytes: usize, heartbeat_duration: Duration) -> f64 {
    let tps = Duration::from_secs(1).as_secs_f64() / heartbeat_duration.as_secs_f64();
    tps * message_size_bytes.into_f64()
}
