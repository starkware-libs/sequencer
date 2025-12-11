use std::time::Duration;

use apollo_metrics::metrics::LossyIntoF64;
use sysinfo::{Pid, System};
use tokio::time::interval;
use tracing::warn;

use crate::metrics::{
    SYSTEM_AVAILABLE_MEMORY_BYTES,
    SYSTEM_CPU_COUNT,
    SYSTEM_PROCESS_CPU_USAGE_PERCENT,
    SYSTEM_PROCESS_MEMORY_USAGE_BYTES,
    SYSTEM_PROCESS_VIRTUAL_MEMORY_USAGE_BYTES,
    SYSTEM_TOTAL_MEMORY_BYTES,
    SYSTEM_USED_MEMORY_BYTES,
};

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

pub async fn monitor_process_metrics(interval_seconds: u64) {
    let mut interval = interval(Duration::from_secs(interval_seconds));
    let current_pid = sysinfo::get_current_pid().expect("Failed to get current process PID");

    struct State {
        system: System,
    }

    let mut state = Some(State { system: System::new_all() });

    loop {
        interval.tick().await;

        let mut passed_state = state.take().unwrap();
        // the metrics update need to be done in a blocking context to avoid slowing down tokio
        // threads
        state = tokio::task::spawn_blocking(move || {
            collect_system_and_process_metrics(&mut passed_state.system, current_pid);
            Some(passed_state)
        })
        .await
        .unwrap();
    }
}
