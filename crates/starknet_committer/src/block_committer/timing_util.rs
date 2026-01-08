use std::fs::{self, File};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use csv::Writer;
use serde::{Deserialize, Serialize};
use starknet_api::hash::HashOutput;
use starknet_types_core::felt::Felt;
use tracing::info;

use crate::block_committer::input::Input;
use crate::db::facts_db::types::FactsDbInitialRead;

pub type FactsDbInputImpl = Input<FactsDbInitialRead>;

#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    EndToEnd(usize),
    Read(usize),
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
            Action::EndToEnd(_) => &mut self.block_timer,
            Action::Read(_) => &mut self.read_timer,
            Action::Compute => &mut self.compute_timer,
            Action::Write => &mut self.writer_timer,
        }
    }

    pub fn start_measurement(&mut self, action: Action) {
        *self.get_mut_timers(&action) = Some(Instant::now());
    }

    pub fn stop_measurement(&mut self, action: &Action) -> Duration {
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

    fn stop_measurement(&mut self, action: Action) {
        self.block_timers().stop_measurement(&action);
    }
}

#[derive(Default)]
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
    pub fn update_after_action(&mut self, action: &Action, duration_in_millis: u64) {
        match action {
            Action::Read(facts_count) => {
                self.read_duration = duration_in_millis;
                self.n_read_facts = *facts_count;
            }
            Action::Compute => {
                self.compute_duration = duration_in_millis;
            }
            Action::Write => {
                self.write_duration = duration_in_millis;
            }
            Action::EndToEnd(facts_count) => {
                self.block_duration = duration_in_millis;
                self.n_new_facts = *facts_count;
                self.time_of_measurement =
                    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
            }
        }
    }
}

pub struct TimeMeasurement {
    pub block_timers: BlockTimers,
    pub total_time: u64, // Total duration of all blocks (milliseconds).
    pub block_measurements: Vec<BlockMeasurement>,
    pub facts_in_db: Vec<usize>, // Number of facts in the DB prior to the current block.
    pub block_number: usize,
    pub total_facts: usize,

    // Storage related statistics.
    pub storage_stat_columns: Vec<&'static str>,
}

impl TimeMeasurementTrait for TimeMeasurement {
    fn block_timers(&mut self) -> &mut BlockTimers {
        &mut self.block_timers
    }

    /// Stop the measurement for the given action and add the duration to the corresponding vector.
    /// facts_count is either the number of facts read from the DB for Read action, or the number of
    /// new facts written to the DB for the Total action.
    /// Assuming the first action to be stopped in each block is Read.
    fn stop_measurement(&mut self, action: Action) {
        let instant_duration = self.block_timers().stop_measurement(&action);
        info!(
            "Time elapsed for {action:?} in iteration {}: {} milliseconds",
            self.n_results(),
            instant_duration.as_millis(),
        );
        let millis: u64 = instant_duration.as_millis().try_into().unwrap();

        if matches!(action, Action::Read(_)) {
            // Create a new `BlockMeasurement` with only the read duration and n_read_facts, and add
            // it to the vector.
            self.block_measurements.push(BlockMeasurement::default());
        }

        let block_measurement =
            self.block_measurements.last_mut().expect("Block measurements should not be empty.");
        block_measurement.update_after_action(&action, millis);

        if let Action::EndToEnd(facts_count) = action {
            self.total_time += millis;
            self.total_facts += facts_count;
            self.block_number += 1;
            self.facts_in_db.push(self.total_facts);
        }
    }
}

impl TimeMeasurement {
    pub fn new(size: usize, storage_stat_columns: Vec<&'static str>) -> Self {
        Self {
            block_timers: BlockTimers::default(),
            total_time: 0,
            block_measurements: Vec::with_capacity(size),
            block_number: 0,
            total_facts: 0,
            facts_in_db: Vec::with_capacity(size),
            storage_stat_columns,
        }
    }

    fn clear_measurements(&mut self) {
        self.block_measurements.clear();
        self.facts_in_db.clear();
    }

    pub fn n_results(&self) -> usize {
        self.block_measurements.len()
    }

    /// Returns the average time per block (milliseconds).
    fn block_average_time(&self) -> f64 {
        #[allow(clippy::as_conversions)]
        {
            self.total_time as f64 / self.n_results() as f64
        }
    }

    /// Returns the average time per fact over a window of `window_size` blocks (microseconds).
    fn average_window_time(&self, window_size: usize) -> Vec<f64> {
        let mut averages = Vec::new(); // In milliseconds.
        // Takes only the full windows, so if the last window is smaller than `window_size`, it is
        // ignored.
        let n_windows = self.n_results() / window_size;
        for i in 0..n_windows {
            let window_start = i * window_size;
            let sum: u64 = self.block_measurements[window_start..window_start + window_size]
                .iter()
                .map(|measurement| measurement.block_duration)
                .sum();
            let sum_of_facts: usize = self.block_measurements
                [window_start..window_start + window_size]
                .iter()
                .map(|measurement| measurement.n_new_facts)
                .sum();
            #[allow(clippy::as_conversions)]
            averages.push(1000.0 * sum as f64 / sum_of_facts as f64);
        }
        averages
    }

    pub fn pretty_print(&self, window_size: usize) {
        if self.n_results() == 0 {
            println!("No measurements were taken.");
            return;
        }

        println!(
            "Total time: {} milliseconds for {} iterations.",
            self.total_time,
            self.n_results()
        );
        println!(
            "Average block time: {:.2} milliseconds.
        ",
            self.block_average_time()
        );

        println!("Average time per window of {window_size} iterations:");
        let means = self.average_window_time(window_size);
        let max = means.iter().cloned().fold(f64::MIN, f64::max);
        // Print a graph visualization of block times.
        for (i, fact_duration) in means.iter().enumerate() {
            let norm = fact_duration / max;
            #[allow(clippy::as_conversions)]
            let width = (norm * 40.0).round() as usize; // up tp 40 characters wide
            let bar = "█".repeat(width.max(1));
            println!("win {i:>4}: {fact_duration:>8.4} microsecond / fact | {bar}");
        }
    }

    pub fn to_csv(
        &mut self,
        path: &str,
        output_dir: &str,
        storage_stat_values: Option<Vec<String>>,
    ) {
        fs::create_dir_all(output_dir).expect("Failed to create output directory.");
        let file =
            File::create(format!("{output_dir}/{path}")).expect("Failed to create CSV file.");
        let mut wtr = Writer::from_writer(file);
        wtr.write_record(
            [
                vec![
                    "block_number",
                    "n_new_facts",
                    "n_read_facts",
                    "initial_facts_in_db",
                    "time_of_measurement",
                    "block_duration_millis",
                    "read_duration_millis",
                    "compute_duration_millis",
                    "write_duration_millis",
                ],
                self.storage_stat_columns.clone(),
            ]
            .concat(),
        )
        .expect("Failed to write CSV header.");
        let n_results = self.n_results();
        let empty_storage_stat_row = vec!["".to_string(); self.storage_stat_columns.len()];
        for i in 0..n_results {
            // The last row in this checkpoint contains the storage statistics.
            let measurement = &self.block_measurements[i];
            let mut record = vec![
                (self.block_number - n_results + i).to_string(),
                measurement.n_new_facts.to_string(),
                measurement.n_read_facts.to_string(),
                self.facts_in_db[i].to_string(),
                measurement.time_of_measurement.to_string(),
                measurement.block_duration.to_string(),
                measurement.read_duration.to_string(),
                measurement.compute_duration.to_string(),
                measurement.write_duration.to_string(),
            ];
            if i == n_results - 1 {
                record
                    .extend(storage_stat_values.clone().unwrap_or(empty_storage_stat_row.clone()));
            } else {
                record.extend(empty_storage_stat_row.clone());
            }
            wtr.write_record(&record).expect("Failed to write CSV record.");
        }
        wtr.flush().expect("Failed to flush CSV writer.");
        self.clear_measurements();
    }

    pub fn try_load_from_checkpoint(&mut self, checkpoint_dir: &str) -> Option<HashOutput> {
        let largest_file_index = get_largest_file_index(checkpoint_dir, "json")?;
        let checkpoint = serde_json::from_str::<Checkpoint>(
            &fs::read_to_string(format!("{checkpoint_dir}/{largest_file_index}.json")).unwrap(),
        )
        .unwrap();
        self.total_facts = checkpoint.total_facts;
        self.block_number = checkpoint.block_number + 1;
        Some(HashOutput(checkpoint.contracts_trie_root_hash))
    }

    /// Save a checkpoint of the benchmark, allowing to resume the benchmark after a crash.
    pub fn save_checkpoint(
        &self,
        checkpoint_dir: &str,
        block_number: usize,
        contracts_trie_root_hash: &HashOutput,
    ) {
        let checkpoint = Checkpoint {
            block_number,
            contracts_trie_root_hash: contracts_trie_root_hash.0,
            total_facts: self.total_facts,
        };
        fs::create_dir_all(checkpoint_dir).expect("Failed to create checkpoint directory.");

        let json = serde_json::to_string_pretty(&checkpoint).unwrap();
        fs::write(format!("{checkpoint_dir}/{block_number}.json"), json).unwrap();
        info!("Saved checkpoint to {checkpoint_dir}/{block_number}.json");
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct Checkpoint {
    block_number: usize,
    contracts_trie_root_hash: Felt,
    total_facts: usize,
}

fn get_largest_file_index(output_dir: &str, file_suffix: &str) -> Option<usize> {
    fs::read_dir(output_dir)
        .ok()?
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().unwrap() == file_suffix)
        .map(|path| path.file_stem().unwrap().to_str().unwrap().parse::<usize>().unwrap())
        .max()
}
