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
    /// milliseconds.
    pub fn attempt_to_stop_measurement(
        &mut self,
        action: &Action,
    ) -> Result<u128, MeasurementNotStartedError> {
        self.get_mut_timers(action).as_mut().map_or_else(
            || {
                error!("attempt_to_stop_measurement called before start_measurement.");
                Err(MeasurementNotStartedError)
            },
            |instant_timer| Ok(instant_timer.elapsed().as_millis()),
        )
    }
}

pub trait TimeMeasurementTrait {
    fn start_measurement(&mut self, action: Action);

    /// Attempts to stop the measurement for the given action and returns the duration in
    /// milliseconds.
    fn attempt_to_stop_measurement(
        &mut self,
        action: Action,
        entries_count: usize,
    ) -> Result<u128, MeasurementNotStartedError>;

    fn set_number_of_modifications(
        &mut self,
        n_storage_tries_modifications: usize,
        n_contracts_trie_modifications: usize,
        n_classes_trie_modifications: usize,
    );

    fn set_number_of_empty_leaves(&mut self, n_empty_leaves: usize);
}

pub struct NoTimeMeasurement;

impl TimeMeasurementTrait for NoTimeMeasurement {
    fn start_measurement(&mut self, _action: Action) {}

    fn attempt_to_stop_measurement(
        &mut self,
        _action: Action,
        _entries_count: usize,
    ) -> Result<u128, MeasurementNotStartedError> {
        Err(MeasurementNotStartedError)
    }

    fn set_number_of_modifications(
        &mut self,
        _n_storage_tries_modifications: usize,
        _n_contracts_trie_modifications: usize,
        _n_classes_trie_modifications: usize,
    ) {
    }

    fn set_number_of_empty_leaves(&mut self, _n_empty_leaves: usize) {}
}

#[derive(Default, Clone)]
pub struct BlockMeasurement {
    pub n_writes: usize,
    pub n_reads: usize,
    pub block_duration: u128,   // Duration of a block commit (milliseconds).
    pub read_duration: u128,    // Duration of a read phase (milliseconds).
    pub compute_duration: u128, // Duration of a computation phase (milliseconds).
    pub write_duration: u128,   // Duration of a write phase (milliseconds).
    pub n_storage_tries_modifications: usize,
    pub n_contracts_trie_modifications: usize,
    pub n_classes_trie_modifications: usize,
    pub n_empty_leaves: usize,
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

    fn attempt_to_stop_measurement(
        &mut self,
        action: Action,
        entries_count: usize,
    ) -> Result<u128, MeasurementNotStartedError> {
        let duration_in_millis = self.block_timers.attempt_to_stop_measurement(&action)?;
        self.block_measurement.update_after_action(&action, entries_count, duration_in_millis);
        Ok(duration_in_millis)
    }

    fn set_number_of_modifications(
        &mut self,
        n_storage_tries_modifications: usize,
        n_contracts_trie_modifications: usize,
        n_classes_trie_modifications: usize,
    ) {
        self.block_measurement.n_storage_tries_modifications = n_storage_tries_modifications;
        self.block_measurement.n_contracts_trie_modifications = n_contracts_trie_modifications;
        self.block_measurement.n_classes_trie_modifications = n_classes_trie_modifications;
    }

    fn set_number_of_empty_leaves(&mut self, n_empty_leaves: usize) {
        self.block_measurement.n_empty_leaves = n_empty_leaves;
    }
}
