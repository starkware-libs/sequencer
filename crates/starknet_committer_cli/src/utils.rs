use std::fs::{self, File};
use std::mem::take;
use std::time::{SystemTime, UNIX_EPOCH};

use csv::Writer;
use serde::{Deserialize, Serialize};
use starknet_api::hash::HashOutput;
use starknet_committer::block_committer::measurements_util::{
    Action,
    BlockMeasurement,
    BlockModificationsCounts,
    MeasurementNotStartedError,
    MeasurementsTrait,
    SingleBlockMeasurements,
};
use starknet_types_core::felt::Felt;
use tracing::info;

#[cfg(test)]
#[path = "utils_test.rs"]
pub mod utils_test;

pub struct BenchmarkMeasurements {
    pub current_measurement: SingleBlockMeasurements,
    pub total_time: u128, // Total duration of all blocks (milliseconds).
    pub block_measurements: Vec<BlockMeasurement>,
    pub initial_db_entry_count: Vec<usize>, /* Number of DB entries prior to the current
                                             * block. */
    pub time_of_measurement: Vec<u128>, /* Milliseconds since epoch (timestamp) of finalizing
                                         * the measurement. */
    pub block_number: usize,
    pub total_db_entry_count: usize,

    // Storage related statistics.
    pub storage_stat_columns: Vec<&'static str>,
}

impl MeasurementsTrait for BenchmarkMeasurements {
    fn start_measurement(&mut self, action: Action) {
        self.current_measurement.start_measurement(action);
    }

    /// Attempts to stop the measurement for the given action and adds the duration to the
    /// corresponding vector. For Read/Write actions, `entries_count` is the number of entries
    /// read from / written to the DB. For other actions, it is ignored.
    /// Returns the duration in milliseconds.
    /// Panics if the measurement was not started.
    fn attempt_to_stop_measurement(
        &mut self,
        action: Action,
        entries_count: usize,
    ) -> Result<u128, MeasurementNotStartedError> {
        let duration_in_millis = self
            .current_measurement
            .attempt_to_stop_measurement(action, entries_count)
            .expect("Failed to stop measurement");
        info!(
            "Time elapsed for {action:?} in iteration {}: {} milliseconds",
            self.n_results(),
            duration_in_millis,
        );

        match action {
            Action::Write => {
                self.initial_db_entry_count.push(self.total_db_entry_count);
                self.total_db_entry_count += entries_count;
            }
            Action::EndToEnd => {
                self.total_time += duration_in_millis;
                self.block_number += 1;
                self.block_measurements.push(take(&mut self.current_measurement.block_measurement));
                self.time_of_measurement
                    .push(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis());
            }
            _ => {}
        }
        Ok(duration_in_millis)
    }

    fn set_number_of_modifications(
        &mut self,
        block_modifications_counts: BlockModificationsCounts,
    ) {
        self.current_measurement.set_number_of_modifications(block_modifications_counts);
    }
}

impl BenchmarkMeasurements {
    pub fn new(size: usize, storage_stat_columns: Vec<&'static str>) -> Self {
        Self {
            current_measurement: SingleBlockMeasurements::default(),
            total_time: 0,
            block_measurements: Vec::with_capacity(size),
            block_number: 4_000_000,
            total_db_entry_count: 0,
            initial_db_entry_count: Vec::with_capacity(size),
            time_of_measurement: Vec::with_capacity(size),
            storage_stat_columns,
        }
    }

    fn clear_measurements(&mut self) {
        self.block_measurements.clear();
        self.initial_db_entry_count.clear();
        self.time_of_measurement.clear();
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

    /// Returns the average time per entry over a window of `window_size` blocks (microseconds).
    fn average_window_time(&self, window_size: usize) -> Vec<f64> {
        let mut averages = Vec::new(); // In milliseconds.
        // Takes only the full windows, so if the last window is smaller than `window_size`, it is
        // ignored.
        let n_windows = self.n_results() / window_size;
        for i in 0..n_windows {
            let window_start = i * window_size;
            let total_duration: u128 = self.block_measurements
                [window_start..window_start + window_size]
                .iter()
                .map(|measurement| measurement.durations.block)
                .sum();
            let sum_of_entries: usize = self.block_measurements
                [window_start..window_start + window_size]
                .iter()
                .map(|measurement| measurement.n_writes)
                .sum();
            #[allow(clippy::as_conversions)]
            averages.push(1000.0 * total_duration as f64 / sum_of_entries as f64);
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
        for (i, entry_duration) in means.iter().enumerate() {
            let norm = entry_duration / max;
            #[allow(clippy::as_conversions)]
            let width = (norm * 40.0).round() as usize; // up tp 40 characters wide
            let bar = "█".repeat(width.max(1));
            println!("win {i:>4}: {entry_duration:>8.4} microsecond / db entry | {bar}");
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
                    "n_writes",
                    "n_reads",
                    "initial_db_entry_count",
                    "time_of_measurement",
                    "block_duration_millis",
                    "read_duration_millis",
                    "compute_duration_millis",
                    "write_duration_millis",
                    "n_storage_tries_modifications",
                    "n_contracts_trie_modifications",
                    "n_classes_trie_modifications",
                    "n_emptied_storage_leaves",
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
                measurement.n_writes.to_string(),
                measurement.n_reads.to_string(),
                self.initial_db_entry_count[i].to_string(),
                self.time_of_measurement[i].to_string(),
                measurement.durations.block.to_string(),
                measurement.durations.read.to_string(),
                measurement.durations.compute.to_string(),
                measurement.durations.write.to_string(),
                measurement.modifications_counts.storage_tries.to_string(),
                measurement.modifications_counts.contracts_trie.to_string(),
                measurement.modifications_counts.classes_trie.to_string(),
                measurement.modifications_counts.emptied_storage_leaves.to_string(),
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
        self.total_db_entry_count = checkpoint.total_db_entry_count;
        self.block_number = checkpoint.block_number;
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
            total_db_entry_count: self.total_db_entry_count,
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
    // TODO(Rotem): remove this serde alias once all benchmarks are updated to use the new name.
    #[serde(alias = "total_facts")]
    total_db_entry_count: usize,
}

fn get_largest_file_index(output_dir: &str, file_suffix: &str) -> Option<usize> {
    fs::read_dir(output_dir)
        .ok()?
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().unwrap() == file_suffix)
        .map(|path| path.file_stem().unwrap().to_str().unwrap().parse::<usize>().unwrap())
        .max()
}
