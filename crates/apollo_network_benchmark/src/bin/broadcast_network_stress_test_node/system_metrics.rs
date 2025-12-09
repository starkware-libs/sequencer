use std::fs;
use std::time::{Duration, Instant};

use apollo_metrics::metrics::LossyIntoF64;
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

const NANOS_PER_SECOND: f64 = 1_000_000_000.0;

/// Linux USER_HZ: the tick rate exposed to userspace via /proc. This is a stable kernel ABI
/// constant that has been 100 on all mainstream architectures for decades.
const CLOCK_TICKS_PER_SEC: u64 = 100;

/// Reads memory info, returning (total, available) in bytes.
///
/// Tries cgroup limits first (container-aware), falls back to /proc/meminfo.
fn get_memory_info() -> Option<(u64, u64)> {
    if let Some(result) = get_cgroup_memory_info() {
        return Some(result);
    }
    get_proc_memory_info()
}

/// Reads cgroup v2 memory limits, then falls back to cgroup v1.
fn get_cgroup_memory_info() -> Option<(u64, u64)> {
    let total = fs::read_to_string("/sys/fs/cgroup/memory.max").ok()?;
    let total = total.trim();
    if total == "max" {
        return None;
    }
    let total_bytes: u64 = total.parse().ok()?;
    let current_bytes: u64 =
        fs::read_to_string("/sys/fs/cgroup/memory.current").ok()?.trim().parse().ok()?;
    let available_bytes = total_bytes.saturating_sub(current_bytes);
    Some((total_bytes, available_bytes))
}

/// Reads /proc/meminfo for system memory stats.
fn get_proc_memory_info() -> Option<(u64, u64)> {
    let content = match fs::read_to_string("/proc/meminfo") {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to read /proc/meminfo: {}", e);
            return None;
        }
    };

    let mut total_kb = None;
    let mut available_kb = None;

    for line in content.lines() {
        if let Some(val) = line.strip_prefix("MemTotal:") {
            total_kb = parse_meminfo_kb(val);
        } else if let Some(val) = line.strip_prefix("MemAvailable:") {
            available_kb = parse_meminfo_kb(val);
        }
        if total_kb.is_some() && available_kb.is_some() {
            break;
        }
    }

    Some((total_kb? * 1024, available_kb? * 1024))
}

/// Parses a value like "  16384000 kB" into the numeric kB value.
fn parse_meminfo_kb(val: &str) -> Option<u64> {
    val.split_whitespace().next()?.parse().ok()
}

/// Reads process CPU ticks (utime + stime) from /proc/self/stat.
fn get_process_cpu_ticks() -> Option<u64> {
    let content = match fs::read_to_string("/proc/self/stat") {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to read /proc/self/stat: {}", e);
            return None;
        }
    };

    // Fields in /proc/self/stat are space-separated, but field 2 (comm) is in parentheses
    // and may contain spaces. Find the closing ')' to skip past it.
    let after_comm = content.rfind(')')?.checked_add(2)?;
    let fields: Vec<&str> = content.get(after_comm..)?.split_whitespace().collect();
    // After comm, fields are 0-indexed from field 3 of the stat file.
    // utime = field 14 (index 11 after comm), stime = field 15 (index 12 after comm)
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    Some(utime + stime)
}

/// Reads process memory from /proc/self/status (VmRSS and VmSize in kB).
/// Returns (rss_bytes, vsize_bytes).
fn get_process_memory() -> Option<(u64, u64)> {
    let content = match fs::read_to_string("/proc/self/status") {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to read /proc/self/status: {}", e);
            return None;
        }
    };

    let mut rss_kb = None;
    let mut vsize_kb = None;

    for line in content.lines() {
        if let Some(val) = line.strip_prefix("VmRSS:") {
            rss_kb = parse_meminfo_kb(val);
        } else if let Some(val) = line.strip_prefix("VmSize:") {
            vsize_kb = parse_meminfo_kb(val);
        }
        if rss_kb.is_some() && vsize_kb.is_some() {
            break;
        }
    }

    Some((rss_kb? * 1024, vsize_kb? * 1024))
}

struct CpuState {
    prev_ticks: u64,
    prev_time: Instant,
}

/// Collects system-wide and process-specific metrics (CPU, memory) by reading /proc directly.
fn collect_system_and_process_metrics(cpu_state: &mut Option<CpuState>) {
    if let Some((total, available)) = get_memory_info() {
        let used = total.saturating_sub(available);
        SYSTEM_TOTAL_MEMORY_BYTES.set(total.into_f64());
        SYSTEM_AVAILABLE_MEMORY_BYTES.set(available.into_f64());
        SYSTEM_USED_MEMORY_BYTES.set(used.into_f64());
    }

    match std::thread::available_parallelism() {
        Ok(count) => SYSTEM_CPU_COUNT.set(count.get().into_f64()),
        Err(e) => warn!("Failed to get CPU count: {}", e),
    }

    if let Some((rss, vsize)) = get_process_memory() {
        SYSTEM_PROCESS_MEMORY_USAGE_BYTES.set(rss.into_f64());
        SYSTEM_PROCESS_VIRTUAL_MEMORY_USAGE_BYTES.set(vsize.into_f64());
    }

    if let Some(current_ticks) = get_process_cpu_ticks() {
        let now = Instant::now();
        if let Some(prev) = cpu_state.as_ref() {
            let tick_delta = current_ticks.saturating_sub(prev.prev_ticks);
            let elapsed = now.duration_since(prev.prev_time);
            let elapsed_secs = elapsed.as_nanos().into_f64() / NANOS_PER_SECOND;
            if elapsed_secs > 0.0 {
                let cpu_seconds = tick_delta.into_f64() / CLOCK_TICKS_PER_SEC.into_f64();
                let cpu_percent = (cpu_seconds / elapsed_secs) * 100.0;
                SYSTEM_PROCESS_CPU_USAGE_PERCENT.set(cpu_percent);
            }
        }
        *cpu_state = Some(CpuState { prev_ticks: current_ticks, prev_time: now });
    }
}

pub async fn monitor_process_metrics(interval_seconds: u64) {
    let mut interval = interval(Duration::from_secs(interval_seconds));

    struct State {
        cpu_state: Option<CpuState>,
    }

    let mut state = Some(State { cpu_state: None });

    loop {
        interval.tick().await;

        let passed_state = state.take();
        state = tokio::task::spawn_blocking(move || {
            let mut state = passed_state.unwrap();
            collect_system_and_process_metrics(&mut state.cpu_state);
            Some(state)
        })
        .await
        .unwrap();
    }
}
