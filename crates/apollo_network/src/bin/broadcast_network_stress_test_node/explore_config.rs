use std::time::Duration;

use crate::args::Args;
use crate::metrics::{get_throughput, seconds_since_epoch};

const EXPLORE_MESSAGE_SIZES_BYTES: [usize; 13] = [
    1 << 10,
    1 << 11,
    1 << 12,
    1 << 13,
    1 << 14,
    1 << 15,
    1 << 16,
    1 << 17,
    1 << 18,
    1 << 19,
    1 << 20,
    1 << 21,
    1 << 22,
];
const EXPLORE_MESSAGE_HEARTBEAT_MILLIS: [u64; 16] =
    [1, 2, 3, 4, 5, 10, 20, 30, 40, 50, 100, 150, 200, 250, 500, 1000];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExplorePhase {
    /// In cooldown period - no broadcasting should occur
    CoolDown,
    /// In running period - broadcasting should occur (if this node is the broadcaster)
    Running,
}

#[derive(Clone)]
pub struct ExploreConfiguration {
    sorted_configurations: Vec<(usize, Duration)>,
    /// The broadcaster configuration index
    configuration_index: usize,
    /// Duration of the Running phase of the cycle
    run_duration_seconds: u64,
    /// Total duration for one complete cycle (cooldown + run_duration_seconds)
    cycle_duration_seconds: u64,
}

impl ExploreConfiguration {
    pub fn new(
        cool_down_duration_seconds: u64,
        run_duration_seconds: u64,
        min_throughput_byte_per_seconds: f64,
        min_message_size_bytes: usize,
    ) -> ExploreConfiguration {
        let mut sorted_configurations = Vec::with_capacity(
            EXPLORE_MESSAGE_SIZES_BYTES.len() * EXPLORE_MESSAGE_HEARTBEAT_MILLIS.len(),
        );
        for message_size in EXPLORE_MESSAGE_SIZES_BYTES {
            for heartbeat_millis in EXPLORE_MESSAGE_HEARTBEAT_MILLIS {
                sorted_configurations.push((message_size, Duration::from_millis(heartbeat_millis)));
            }
        }
        sorted_configurations.retain(|(size, duration)| {
            *size >= min_message_size_bytes
                && get_throughput(*size, *duration) >= min_throughput_byte_per_seconds
        });
        sorted_configurations
            .sort_by_cached_key(|(size, duration)| get_throughput(*size, *duration) as u64);

        let cycle_duration_seconds = cool_down_duration_seconds + run_duration_seconds;

        Self {
            sorted_configurations,
            configuration_index: 0,
            run_duration_seconds,
            cycle_duration_seconds,
        }
    }

    /// Gets the current phase within the current configuration cycle
    pub fn get_current_phase(&self) -> ExplorePhase {
        let now_seconds = seconds_since_epoch();
        let position_in_cycle_seconds = now_seconds % self.cycle_duration_seconds;

        if position_in_cycle_seconds < self.run_duration_seconds {
            ExplorePhase::Running
        } else {
            ExplorePhase::CoolDown
        }
    }

    /// Gets the current message size and duration based on synchronized time
    pub fn get_current_size_and_heartbeat(&mut self) -> (usize, Duration) {
        let config_index = self.configuration_index;
        self.configuration_index += 1;
        if self.configuration_index >= self.sorted_configurations.len() {
            self.configuration_index = 0;
        }
        self.sorted_configurations[config_index]
    }
}

/// Extracts explore mode parameters from arguments with validation
pub fn extract_explore_params(args: &Args) -> (u64, u64, f64, usize) {
    let cool_down = args
        .explore_cool_down_duration_seconds
        .expect("explore_cool_down_duration_seconds required for explore mode");
    let run_duration = args
        .explore_run_duration_seconds
        .expect("explore_run_duration_seconds required for explore mode");
    let min_throughput = args
        .explore_min_throughput_byte_per_seconds
        .expect("explore_min_throughput_byte_per_seconds required for explore mode");
    let min_message_size = args
        .explore_min_message_size_bytes
        .expect("explore_min_message_size_bytes required for explore mode");

    (cool_down, run_duration, min_throughput, min_message_size)
}
