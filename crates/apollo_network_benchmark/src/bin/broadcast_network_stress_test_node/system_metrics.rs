use std::fs;
use std::time::Duration;

use apollo_metrics::metrics::LossyIntoF64;
use sysinfo::{Networks, Pid, System};
use tokio::time::interval;
use tracing::warn;

use crate::metrics::{
    SYSTEM_AVAILABLE_MEMORY_BYTES,
    SYSTEM_CPU_COUNT,
    SYSTEM_NETWORK_BYTES_RECEIVED_CURRENT,
    SYSTEM_NETWORK_BYTES_RECEIVED_TOTAL,
    SYSTEM_NETWORK_BYTES_SENT_CURRENT,
    SYSTEM_NETWORK_BYTES_SENT_TOTAL,
    SYSTEM_PROCESS_CPU_USAGE_PERCENT,
    SYSTEM_PROCESS_MEMORY_USAGE_BYTES,
    SYSTEM_PROCESS_VIRTUAL_MEMORY_USAGE_BYTES,
    SYSTEM_TCP_RETRANSMIT_RATE_PERCENT,
    SYSTEM_TCP_SEGMENTS_OUT,
    SYSTEM_TCP_SEGMENTS_RETRANS,
    SYSTEM_TOTAL_MEMORY_BYTES,
    SYSTEM_USED_MEMORY_BYTES,
};

/// Reads TCP statistics from /proc/net/snmp on Linux systems
/// Returns (segments_out, retransmitted_segments) if successful
fn get_tcp_stats() -> Option<(u64, u64)> {
    let content = match fs::read_to_string("/proc/net/snmp") {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to read /proc/net/snmp: {}", e);
            return None;
        }
    };

    // Parse Tcp statistics
    // Format is two lines: Tcp: <keys>\nTcp: <values>
    let lines: Vec<&str> = content.lines().collect();
    for i in 0..lines.len().saturating_sub(1) {
        if lines[i].starts_with("Tcp:") && lines[i + 1].starts_with("Tcp:") {
            let keys: Vec<&str> = lines[i].split_whitespace().skip(1).collect();
            let values: Vec<&str> = lines[i + 1].split_whitespace().skip(1).collect();

            let mut out_segs = None;
            let mut retrans_segs = None;

            for (key, val) in keys.iter().zip(values.iter()) {
                match *key {
                    "OutSegs" => out_segs = val.parse().ok(),
                    "RetransSegs" => retrans_segs = val.parse().ok(),
                    _ => {}
                }
            }

            if out_segs.is_none() || retrans_segs.is_none() {
                warn!(
                    "Could not find OutSegs or RetransSegs in /proc/net/snmp. Found keys: {:?}",
                    keys
                );
            }

            return Some((out_segs?, retrans_segs?));
        }
    }

    warn!("Could not find Tcp: section in /proc/net/snmp");
    None
}

/// Collects system-wide and process-specific metrics (CPU, memory)
fn collect_system_and_process_metrics(system: &mut System, current_pid: Pid) {
    system.refresh_all();
    let total_memory: f64 = system.total_memory().into_f64();
    let available_memory: f64 = system.available_memory().into_f64();
    let used_memory: f64 = system.used_memory().into_f64();
    let cpu_count: f64 = system.cpus().len().into_f64();

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
}

/// Collects network interface metrics (bytes sent/received)
fn collect_network_metrics(networks: &mut Networks) {
    networks.refresh(false);

    let mut total_bytes_sent: u64 = 0;
    let mut total_bytes_received: u64 = 0;
    let mut current_bytes_sent: u64 = 0;
    let mut current_bytes_received: u64 = 0;

    for (interface_name, data) in networks.iter() {
        // Skip virtual interfaces used for traffic control and loopback to avoid
        // double-counting
        if interface_name == "lo" || interface_name.starts_with("ifb") {
            continue;
        }

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

/// Collects TCP statistics and calculates retransmit rate
fn collect_tcp_metrics(
    prev_tcp_out_segs: &mut Option<u64>,
    prev_tcp_retrans_segs: &mut Option<u64>,
) {
    // Collect TCP statistics and calculate retransmit rate
    if let Some((curr_out_segs, curr_retrans_segs)) = get_tcp_stats() {
        // Update total counters
        SYSTEM_TCP_SEGMENTS_OUT.set(curr_out_segs.into_f64());
        SYSTEM_TCP_SEGMENTS_RETRANS.set(curr_retrans_segs.into_f64());

        // Calculate retransmit rate based on delta since last measurement
        if let (Some(prev_out), Some(prev_retrans)) = (*prev_tcp_out_segs, *prev_tcp_retrans_segs) {
            let delta_out = curr_out_segs.saturating_sub(prev_out);
            let delta_retrans = curr_retrans_segs.saturating_sub(prev_retrans);

            let retransmit_rate_percent =
                if delta_out > 0 { (delta_retrans as f64 / delta_out as f64) * 100.0 } else { 0.0 };

            SYSTEM_TCP_RETRANSMIT_RATE_PERCENT.set(retransmit_rate_percent);
        }

        // Update previous values for next iteration
        *prev_tcp_out_segs = Some(curr_out_segs);
        *prev_tcp_retrans_segs = Some(curr_retrans_segs);
    }
    // Note: get_tcp_stats() logs detailed warnings on failure
}

pub async fn monitor_process_metrics(interval_seconds: u64) {
    let mut interval = interval(Duration::from_secs(interval_seconds));
    let current_pid = sysinfo::get_current_pid().expect("Failed to get current process PID");

    struct State {
        system: System,
        networks: Networks,
        prev_tcp_out_segs: Option<u64>,
        prev_tcp_retrans_segs: Option<u64>,
    }

    let mut state = Some(State {
        system: System::new_all(),
        networks: Networks::new_with_refreshed_list(),
        prev_tcp_out_segs: None,
        prev_tcp_retrans_segs: None,
    });

    loop {
        interval.tick().await;

        let passed_state = state.take();
        // the metrics update need to be done in a blocking context to avoid slowing down tokio
        // threads
        state = tokio::task::spawn_blocking(move || {
            let mut state = passed_state.unwrap();
            collect_system_and_process_metrics(&mut state.system, current_pid);
            collect_network_metrics(&mut state.networks);
            collect_tcp_metrics(&mut state.prev_tcp_out_segs, &mut state.prev_tcp_retrans_segs);
            Some(state)
        })
        .await
        .unwrap();
    }
}
