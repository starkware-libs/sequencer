use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Action {
    EndToEnd,
    Read,
    Compute,
    Write,
}

#[derive(Default)]
pub struct BlockTimers {
    pub block_timer: Option<Instant>,
    pub read_timer: Option<Instant>,
    pub compute_timer: Option<Instant>,
    pub writer_timer: Option<Instant>,
}

impl BlockTimers {
    fn get_mut_timers(&mut self, action: &Action) -> &mut Option<Instant> {
        match action {
            Action::EndToEnd => &mut self.block_timer,
            Action::Read => &mut self.read_timer,
            Action::Compute => &mut self.compute_timer,
            Action::Write => &mut self.writer_timer,
        }
    }

    pub fn start_measurement(&mut self, action: Action) {
        *self.get_mut_timers(&action) = Some(Instant::now());
    }

    /// Stops the measurement for the given action and returns the duration in milliseconds.
    pub fn stop_measurement(&mut self, action: &Action) -> u128 {
        let instant_timer = self
            .get_mut_timers(action)
            .as_mut()
            .expect("stop_measurement called before start_measurement");
        instant_timer.elapsed().as_millis()
    }
}

pub trait TimeMeasurementTrait {
    fn start_measurement(&mut self, action: Action);

    /// Stops the measurement for the given action and returns the duration in milliseconds.
    fn stop_measurement(&mut self, action: Action, entries_count: usize) -> u128;
}

#[derive(Default, Clone)]
pub struct BlockMeasurement {
    pub n_writes: usize,
    pub n_reads: usize,
    pub block_duration: u128,   // Duration of a block commit (milliseconds).
    pub read_duration: u128,    // Duration of a read phase (milliseconds).
    pub compute_duration: u128, // Duration of a computation phase (milliseconds).
    pub write_duration: u128,   // Duration of a write phase (milliseconds).
    pub time_of_measurement: u128, /* Milliseconds since epoch (timestamp) of finalizing the
                                 * measurement. */
}

impl BlockMeasurement {
    pub fn update_after_action(
        &mut self,
        action: &Action,
        entries_count: usize,
        duration_in_millis: u128,
    ) {
        match action {
            Action::Read => {
                self.read_duration = duration_in_millis;
                self.n_reads = entries_count;
            }
            Action::Compute => {
                self.compute_duration = duration_in_millis;
            }
            Action::Write => {
                self.write_duration = duration_in_millis;
                self.n_writes = entries_count;
            }
            Action::EndToEnd => {
                self.block_duration = duration_in_millis;
                self.time_of_measurement =
                    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
            }
        }
    }
}

#[derive(Default)]
pub struct SingleBlockTimeMeasurement {
    pub block_timers: BlockTimers,
    pub block_measurement: BlockMeasurement,
}

impl TimeMeasurementTrait for SingleBlockTimeMeasurement {
    fn start_measurement(&mut self, action: Action) {
        self.block_timers.start_measurement(action);
    }

    fn stop_measurement(&mut self, action: Action, entries_count: usize) -> u128 {
        let duration_in_millis = self.block_timers.stop_measurement(&action);
        self.block_measurement.update_after_action(&action, entries_count, duration_in_millis);
        duration_in_millis
    }
}
