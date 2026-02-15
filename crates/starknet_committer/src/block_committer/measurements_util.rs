use std::time::Instant;

use tracing::error;

#[derive(Debug)]
pub struct MeasurementNotStartedError;

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

    /// Attempts to stop the measurement for the given action and returns the duration in
    /// seconds.
    pub fn attempt_to_stop_measurement(
        &mut self,
        action: &Action,
    ) -> Result<f64, MeasurementNotStartedError> {
        self.get_mut_timers(action).as_mut().map_or_else(
            || {
                error!("attempt_to_stop_measurement called before start_measurement.");
                Err(MeasurementNotStartedError)
            },
            |instant_timer| Ok(instant_timer.elapsed().as_secs_f64()),
        )
    }
}

pub trait MeasurementsTrait {
    fn start_measurement(&mut self, action: Action);

    /// Attempts to stop the measurement for the given action and returns the duration in
    /// seconds.
    fn attempt_to_stop_measurement(
        &mut self,
        action: Action,
        entries_count: usize,
    ) -> Result<f64, MeasurementNotStartedError>;

    fn set_number_of_modifications(&mut self, block_modifications_counts: BlockModificationsCounts);
}

pub struct NoMeasurements;

impl MeasurementsTrait for NoMeasurements {
    fn start_measurement(&mut self, _action: Action) {}

    fn attempt_to_stop_measurement(
        &mut self,
        _action: Action,
        _entries_count: usize,
    ) -> Result<f64, MeasurementNotStartedError> {
        Err(MeasurementNotStartedError)
    }

    fn set_number_of_modifications(
        &mut self,
        _block_modifications_counts: BlockModificationsCounts,
    ) {
    }
}

#[derive(Default, Clone)]
pub struct BlockDurations {
    pub block: f64,   // Duration of a block commit (seconds).
    pub read: f64,    // Duration of a read phase (seconds).
    pub compute: f64, // Duration of a computation phase (seconds).
    pub write: f64,   // Duration of a write phase (seconds).
}

#[derive(Default, Clone, Debug, PartialEq, Eq)]
pub struct BlockModificationsCounts {
    pub storage_tries: usize,
    pub contracts_trie: usize,
    pub classes_trie: usize,
    pub emptied_storage_leaves: usize,
}

impl BlockModificationsCounts {
    pub fn total(&self) -> usize {
        self.storage_tries + self.contracts_trie + self.classes_trie
    }
}

#[derive(Default, Clone)]
pub struct BlockMeasurement {
    pub n_writes: usize,
    pub n_reads: usize,
    pub durations: BlockDurations,
    pub modifications_counts: BlockModificationsCounts,
}

impl BlockMeasurement {
    pub fn update_after_action(
        &mut self,
        action: &Action,
        entries_count: usize,
        duration_in_seconds: f64,
    ) {
        match action {
            Action::Read => {
                self.durations.read = duration_in_seconds;
                self.n_reads = entries_count;
            }
            Action::Compute => {
                self.durations.compute = duration_in_seconds;
            }
            Action::Write => {
                self.durations.write = duration_in_seconds;
                self.n_writes = entries_count;
            }
            Action::EndToEnd => {
                self.durations.block = duration_in_seconds;
            }
        }
    }
}

#[derive(Default)]
pub struct SingleBlockMeasurements {
    pub block_timers: BlockTimers,
    pub block_measurement: BlockMeasurement,
}

impl MeasurementsTrait for SingleBlockMeasurements {
    fn start_measurement(&mut self, action: Action) {
        self.block_timers.start_measurement(action);
    }

    fn attempt_to_stop_measurement(
        &mut self,
        action: Action,
        entries_count: usize,
    ) -> Result<f64, MeasurementNotStartedError> {
        let duration_in_seconds = self.block_timers.attempt_to_stop_measurement(&action)?;
        self.block_measurement.update_after_action(&action, entries_count, duration_in_seconds);
        Ok(duration_in_seconds)
    }

    fn set_number_of_modifications(
        &mut self,
        block_modifications_counts: BlockModificationsCounts,
    ) {
        self.block_measurement.modifications_counts = block_modifications_counts;
    }
}
