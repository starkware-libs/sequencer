use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Action<T = ()> {
    EndToEnd,
    /// When stopping, T is usize representing the number of facts read from the DB.
    Read(T),
    Compute,
    /// When stopping, T is usize representing the number of new facts written to the DB.
    Write(T),
}

#[derive(Default)]
pub struct BlockTimers {
    pub block_timer: Option<Instant>,
    pub read_timer: Option<Instant>,
    pub compute_timer: Option<Instant>,
    pub writer_timer: Option<Instant>,
}

impl BlockTimers {
    fn get_mut_timers<T>(&mut self, action: &Action<T>) -> &mut Option<Instant> {
        match action {
            Action::EndToEnd => &mut self.block_timer,
            Action::Read(_) => &mut self.read_timer,
            Action::Compute => &mut self.compute_timer,
            Action::Write(_) => &mut self.writer_timer,
        }
    }

    pub fn start_measurement(&mut self, action: Action) {
        *self.get_mut_timers(&action) = Some(Instant::now());
    }

    pub fn stop_measurement(&mut self, action: &Action<usize>) -> Duration {
        let instant_timer = self
            .get_mut_timers(action)
            .as_mut()
            .expect("stop_measurement called before start_measurement");
        instant_timer.elapsed()
    }
}

pub trait TimeMeasurementTrait {
    fn block_timers(&mut self) -> &mut BlockTimers;

    fn start_measurement(&mut self, action: Action) {
        self.block_timers().start_measurement(action);
    }

    fn stop_measurement(&mut self, action: Action<usize>) {
        self.block_timers().stop_measurement(&action);
    }
}

#[derive(Default, Clone)]
pub struct BlockMeasurement {
    pub n_new_facts: usize,
    pub n_read_facts: usize,
    pub block_duration: u64,   // Duration of a block commit (milliseconds).
    pub read_duration: u64,    // Duration of a block facts read (milliseconds).
    pub compute_duration: u64, // Duration of a block new facts computation (milliseconds).
    pub write_duration: u64,   // Duration of a block new facts write (milliseconds).
    pub time_of_measurement: u128, /* Milliseconds since epoch (timestamp) of the measurement
                                * for each action. */
}

impl BlockMeasurement {
    pub fn update_after_action(&mut self, action: &Action<usize>, duration_in_millis: u64) {
        match action {
            Action::Read(facts_count) => {
                self.read_duration = duration_in_millis;
                self.n_read_facts = *facts_count;
            }
            Action::Compute => {
                self.compute_duration = duration_in_millis;
            }
            Action::Write(facts_count) => {
                self.write_duration = duration_in_millis;
                self.n_new_facts = *facts_count;
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
    fn block_timers(&mut self) -> &mut BlockTimers {
        &mut self.block_timers
    }
}
