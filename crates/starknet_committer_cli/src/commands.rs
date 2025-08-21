#![allow(clippy::as_conversions)]

use std::fs::{self, File};
use std::time::Instant;

use csv::Writer;
use rand::rngs::StdRng;
use rand::SeedableRng;
use starknet_committer::block_committer::commit::commit_block;
use starknet_committer::block_committer::input::{ConfigImpl, Input};
use starknet_committer::block_committer::state_diff_generator::generate_random_state_diff;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia_storage::map_storage::MapStorage;
use tracing::info;

pub type InputImpl = Input<ConfigImpl>;
struct TimeMeasurement {
    timer: Option<Instant>,
    total_time: u128,             // Total duration of all blocks (milliseconds).
    per_fact_durations: Vec<u64>, // Average duration (microseconds) per new fact in a block.
    n_facts: Vec<usize>,
    block_durations: Vec<u64>, // Duration of a block (milliseconds).
    facts_in_db: Vec<usize>,
    total_facts: usize,
}

impl TimeMeasurement {
    fn new(n_iterations: usize) -> Self {
        Self {
            timer: None,
            total_time: 0,
            per_fact_durations: Vec::with_capacity(n_iterations),
            n_facts: Vec::with_capacity(n_iterations),
            block_durations: Vec::with_capacity(n_iterations),
            facts_in_db: Vec::with_capacity(n_iterations),
            total_facts: 0,
        }
    }

    fn start_measurement(&mut self) {
        self.timer = Some(Instant::now());
    }

    fn stop_measurement(&mut self, facts_count: usize) {
        let duration =
            self.timer.expect("stop_measurement called before start_measurement").elapsed();
        info!(
            "Time elapsed for iteration {}: {} milliseconds",
            self.n_results(),
            duration.as_millis()
        );
        self.total_time += duration.as_millis();
        self.per_fact_durations
            .push(duration.div_f32(facts_count as f32).as_micros().try_into().unwrap());
        self.block_durations.push(duration.as_millis().try_into().unwrap());
        self.n_facts.push(facts_count);
        self.facts_in_db.push(self.total_facts);
        self.total_facts += facts_count;
    }

    fn n_results(&self) -> usize {
        self.block_durations.len()
    }

    /// Returns the average time per block (milliseconds).
    fn block_average_time(&self) -> f64 {
        self.total_time as f64 / self.n_results() as f64
    }

    /// Returns the average time per fact over a window of `window_size` blocks (microseconds).
    fn average_window_time(&self, window_size: usize) -> Vec<f64> {
        let mut averages = Vec::new(); // In milliseconds.
        // Takes only the full windows, so if the last window is smaller than `window_size`, it is
        // ignored.
        let n_windows = self.n_results() / window_size;
        for i in 0..n_windows {
            let window_start = i * window_size;
            let sum: u64 =
                self.block_durations[window_start..window_start + window_size].iter().sum();
            let sum_of_facts: usize =
                self.n_facts[window_start..window_start + window_size].iter().sum();
            averages.push(1000.0 * sum as f64 / sum_of_facts as f64);
        }
        averages
    }

    fn pretty_print(&self, window_size: usize) {
        if self.n_results() == 0 {
            println!("No measurements were taken.");
            return;
        }

        println!(
            "Total time: {} milliseconds for {} iterations",
            self.total_time,
            self.n_results()
        );
        println!("Average block time: {:.2} milliseconds", self.block_average_time());

        println!("Average time per window of {window_size} iterations:");
        let means = self.average_window_time(window_size);
        let max = means.iter().cloned().fold(f64::MIN, f64::max);
        // Print a graph visualization of block times.
        for (i, fact_duration) in means.iter().enumerate() {
            let norm = fact_duration / max;
            let width = (norm * 40.0).round() as usize; // up tp 40 characters wide
            let bar = "█".repeat(width.max(1));
            println!("win {i:>4}: {fact_duration:>8.4} microsecond / fact | {bar}");
        }
    }

    fn to_csv(&self, path: &str, output_dir: &str) {
        let _ = fs::create_dir_all(output_dir);
        let file =
            File::create(format!("{output_dir}/{path}")).expect("Failed to create CSV file.");
        let mut wtr = Writer::from_writer(file);
        wtr.write_record([
            "block_number",
            "n_facts",
            "facts_in_db",
            "time_per_fact_micros",
            "block_duration_millis",
        ])
        .expect("Failed to write CSV header.");
        for (i, (((&per_fact, &n_facts), &duration), &facts_in_db)) in self
            .per_fact_durations
            .iter()
            .zip(self.n_facts.iter())
            .zip(self.block_durations.iter())
            .zip(self.facts_in_db.iter())
            .enumerate()
        {
            wtr.write_record(&[
                i.to_string(),
                n_facts.to_string(),
                facts_in_db.to_string(),
                per_fact.to_string(),
                duration.to_string(),
            ])
            .expect("Failed to write CSV record.");
        }
        wtr.flush().expect("Failed to flush CSV writer.");
    }
}

pub async fn run_storage_benchmark(n_iterations: usize, output_dir: &str) {
    let seed = 42_u64; // Constant seed for reproducibility

    let mut rng = StdRng::seed_from_u64(seed);
    let mut time_measurement = TimeMeasurement::new(n_iterations);

    let mut storage = MapStorage::default();
    let mut contracts_trie_root_hash = HashOutput::default();
    let mut classes_trie_root_hash = HashOutput::default();

    for i in 0..n_iterations {
        info!("Committer storage benchmark iteration {}/{}", i + 1, n_iterations);
        let input = InputImpl {
            state_diff: generate_random_state_diff(&mut rng),
            contracts_trie_root_hash,
            classes_trie_root_hash,
            config: ConfigImpl::default(),
        };

        time_measurement.start_measurement();
        let filled_forest =
            commit_block(input, &mut storage).await.expect("Failed to commit the given block.");
        let n_new_facts = filled_forest.write_to_storage(&mut storage);

        time_measurement.stop_measurement(n_new_facts);

        contracts_trie_root_hash = filled_forest.get_contract_root_hash();
        classes_trie_root_hash = filled_forest.get_compiled_class_root_hash();
    }

    time_measurement.pretty_print(50);
    time_measurement.to_csv(&format!("{n_iterations}.csv"), output_dir);
}
