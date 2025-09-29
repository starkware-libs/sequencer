use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use apollo_metrics::define_metrics;
use apollo_metrics::metrics::LossyIntoF64;
use apollo_network::network_manager::metrics::{
    BroadcastNetworkMetrics,
    EventMetrics,
    NetworkMetrics,
    SqmrNetworkMetrics,
    EVENT_TYPE_LABELS,
    NETWORK_BROADCAST_DROP_LABELS,
};
use libp2p::gossipsub::{Sha256Topic, Topic};
use sysinfo::{Networks, System};
use tokio::time::interval;
use tracing::warn;

use crate::converters::StressTestMessage;

lazy_static::lazy_static! {
    pub static ref TOPIC: Sha256Topic = Topic::new("stress_test_topic".to_string());
}

define_metrics!(
    Infra => {
        MetricGauge { BROADCAST_MESSAGE_HEARTBEAT_MILLIS, "broadcast_message_theoretical_heartbeat_millis", "The number of ms we sleep between each two consecutive broadcasts" },
        MetricGauge { BROADCAST_MESSAGE_THROUGHPUT, "broadcast_message_theoretical_throughput", "Throughput in bytes/second of the broadcasted " },
        MetricGauge { BROADCAST_MESSAGE_BYTES, "broadcast_message_bytes", "Size of the stress test sent message in bytes" },
        MetricCounter { BROADCAST_MESSAGE_COUNT, "broadcast_message_count", "Number of stress test messages sent via broadcast", init = 0 },
        MetricCounter { BROADCAST_MESSAGE_BYTES_SUM, "broadcast_message_bytes_sum", "Sum of the stress test messages sent via broadcast", init = 0 },
        MetricHistogram { BROADCAST_MESSAGE_SEND_DELAY_SECONDS, "broadcast_message_send_delay_seconds", "Message sending delay in seconds" },

        MetricGauge { RECEIVE_MESSAGE_BYTES, "receive_message_bytes", "Size of the stress test received message in bytes" },
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

        MetricCounter { NETWORK_RESET_TOTAL, "network_reset_total", "Total number of network resets performed", init = 0 },
        LabeledMetricCounter { NETWORK_DROPPED_BROADCAST_MESSAGES, "network_dropped_broadcast_messages", "Number of dropped broadcast messages by reason", init = 0, labels = NETWORK_BROADCAST_DROP_LABELS },
        LabeledMetricCounter { NETWORK_EVENT_COUNTER, "network_event_counter", "Network events counter by type", init = 0, labels = EVENT_TYPE_LABELS },
    },
);

pub fn update_broadcast_metrics(message_size_bytes: usize, broadcast_heartbeat: Duration) {
    BROADCAST_MESSAGE_HEARTBEAT_MILLIS.set(broadcast_heartbeat.as_millis().into_f64());
    BROADCAST_MESSAGE_THROUGHPUT.set(get_throughput(message_size_bytes, broadcast_heartbeat));
}

pub fn receive_stress_test_message(received_message: Vec<u8>) {
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
        num_sent_broadcast_messages: NETWORK_STRESS_TEST_SENT_MESSAGES,
        num_dropped_broadcast_messages: NETWORK_DROPPED_BROADCAST_MESSAGES,
        num_received_broadcast_messages: NETWORK_STRESS_TEST_RECEIVED_MESSAGES,
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

    NetworkMetrics {
        num_connected_peers: NETWORK_CONNECTED_PEERS,
        num_blacklisted_peers: NETWORK_BLACKLISTED_PEERS,
        broadcast_metrics_by_topic: Some(broadcast_metrics_by_topic),
        sqmr_metrics: Some(sqmr_metrics),
        event_metrics: Some(event_metrics),
    }
}

pub async fn monitor_process_metrics(interval_seconds: u64) {
    let mut interval = interval(Duration::from_secs(interval_seconds));
    let current_pid = sysinfo::get_current_pid().expect("Failed to get current process PID");

    // Initialize networks for network interface monitoring
    let mut networks = Networks::new_with_refreshed_list();

    // Initialize empty system for CPU monitoring - we'll refresh only what we need
    let mut system = System::new_all();

    loop {
        interval.tick().await;

        // Refresh only the specific data we need
        // system.refresh_memory_specifics(MemoryRefreshKind::new().with_ram());
        // system.refresh_cpu_specifics(CpuRefreshKind::new().with_cpu_usage());
        // system.refresh_processes_specifics(
        //     ProcessesToUpdate::Some(&[current_pid]),
        //     false,
        //     ProcessRefreshKind::everything(),
        // );
        // system.refresh_specifics(
        //     RefreshKind::new()
        //         .with_cpu(CpuRefreshKind::everything())
        //         .with_memory(MemoryRefreshKind::everything())
        //         .with_processes(ProcessRefreshKind::new().spe),
        // );
        system.refresh_all();
        let total_memory: f64 = system.total_memory().into_f64();
        let available_memory: f64 = system.available_memory().into_f64();
        let used_memory: f64 = system.used_memory().into_f64();
        let cpu_count: f64 = system.cpus().len().into_f64();
        // let load_avg: f64 = system.load_average().one.into_f64();

        SYSTEM_TOTAL_MEMORY_BYTES.set(total_memory);
        SYSTEM_AVAILABLE_MEMORY_BYTES.set(available_memory);
        SYSTEM_USED_MEMORY_BYTES.set(used_memory);
        SYSTEM_CPU_COUNT.set(cpu_count);

        if let Some(process) = system.process(current_pid) {
            let cpu_usage: f64 = process.cpu_usage().into();
            let memory_usage: f64 = process.memory().into_f64();
            let virtual_memory_usage: f64 = process.virtual_memory().into_f64();

            SYSTEM_PROCESS_CPU_USAGE_PERCENT.set(cpu_usage);
            SYSTEM_PROCESS_MEMORY_USAGE_BYTES.set(memory_usage);
            SYSTEM_PROCESS_VIRTUAL_MEMORY_USAGE_BYTES.set(virtual_memory_usage);
        } else {
            warn!("Could not find process information for PID: {}", current_pid);
        }

        // Refresh network statistics and collect metrics
        networks.refresh(false);

        let mut total_bytes_sent: u64 = 0;
        let mut total_bytes_received: u64 = 0;
        let mut current_bytes_sent: u64 = 0;
        let mut current_bytes_received: u64 = 0;

        for (_interface_name, data) in &networks {
            total_bytes_sent += data.total_transmitted();
            total_bytes_received += data.total_received();
            current_bytes_sent += data.transmitted();
            current_bytes_received += data.received();
        }

        SYSTEM_NETWORK_BYTES_SENT_TOTAL.set(total_bytes_sent.into_f64());
        SYSTEM_NETWORK_BYTES_RECEIVED_TOTAL.set(total_bytes_received.into_f64());
        SYSTEM_NETWORK_BYTES_SENT_CURRENT.set(current_bytes_sent.into_f64());
        SYSTEM_NETWORK_BYTES_RECEIVED_CURRENT.set(current_bytes_received.into_f64());
    }
}
