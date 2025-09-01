use std::time::Instant;

use rand::rngs::StdRng;
use rand::SeedableRng;
use starknet_api::core::ContractAddress;
use starknet_committer::block_committer::commit::commit_block;
use starknet_committer::block_committer::input::{Config, ConfigImpl};
use starknet_committer::block_committer::state_diff_generator::{
    generate_random_state_diff,
    CONTRACT_ADDRESS,
};
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::types::NodeIndex;
use starknet_patricia_storage::map_storage::MapStorage;
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::reload::Handle;
use tracing_subscriber::Registry;

use crate::committer_cli::filled_tree_output::filled_forest::SerializedForest;
use crate::committer_cli::parse_input::cast::{CommitterInputImpl, InputImpl};
use crate::committer_cli::parse_input::raw_input::RawInput;
use crate::shared_utils::read::{load_input, write_to_file};

pub async fn parse_and_commit(
    input_path: String,
    output_path: String,
    log_filter_handle: Handle<LevelFilter, Registry>,
) {
    let CommitterInputImpl { input, storage } = load_input::<RawInput>(input_path)
        .try_into()
        .expect("Failed to convert RawInput to InputImpl.");
    info!(
        "Parsed committer input successfully. Original Contracts Trie Root Hash: {:?},
    Original Classes Trie Root Hash: {:?}",
        input.contracts_trie_root_hash, input.classes_trie_root_hash,
    );
    // Set the given log level if handle is passed.
    log_filter_handle
        .modify(|filter| *filter = input.config.logger_level())
        .expect("Failed to set the log level.");
    commit(input, output_path, storage).await;
}

pub async fn commit(input: InputImpl, output_path: String, storage: MapStorage) {
    let serialized_filled_forest = SerializedForest(
        commit_block(input, &storage).await.expect("Failed to commit the given block."),
    );
    let output = serialized_filled_forest.forest_to_output();
    write_to_file(&output_path, &output);
    info!(
        "Successfully committed given block. Updated Contracts Trie Root Hash: {:?},
    Updated Classes Trie Root Hash: {:?}",
        output.contract_storage_root_hash, output.compiled_class_root_hash,
    );
}

struct TimeMeasurement {
    timer: Option<Instant>,
    total_time: u128,
    results: Vec<f64>,
}

impl TimeMeasurement {
    fn new() -> Self {
        Self { timer: None, total_time: 0, results: Vec::new() }
    }

    fn start_measurement(&mut self) {
        self.timer = Some(Instant::now());
    }

    fn stop_measurement(&mut self, norm: usize) {
        let duration = self.timer.expect("stop measurement before starting").elapsed();
        info!(
            "Time elapsed for iteration {}: {} milliseconds",
            self.n_results(),
            duration.as_millis()
        );
        self.total_time += duration.as_millis();
        self.results.push(duration.as_micros() as f64 / norm as f64);
    }

    fn n_results(&self) -> usize {
        self.results.len()
    }

    fn average_time(&self) -> f64 {
        self.total_time as f64 / self.n_results() as f64
    }

    fn average_window_time(&self, window_size: usize) -> Vec<f64> {
        let mut averages = Vec::new();
        // Takes only the full windows, so if the last window is smaller than `window_size`, it is
        // ignored.
        let n_windows = self.n_results() / window_size;
        for i in 0..n_windows {
            let window_start = i * window_size;
            let sum: f64 = self.results[window_start..window_start + window_size].iter().sum();
            averages.push(sum / window_size as f64);
        }
        averages
    }

    fn pretty_print(&self, window_size: usize) {
        if self.n_results() == 0 {
            info!("No measurements were taken.");
            return;
        }

        info!("Total time: {} milliseconds for {} iterations", self.total_time, self.n_results());
        info!("Average time: {:.2} milliseconds", self.average_time());

        info!("Average time per window of {window_size} iterations:");
        let means = self.average_window_time(window_size);
        let max = means.iter().cloned().fold(f64::MIN, f64::max);
        for (i, m) in means.iter().enumerate() {
            let norm = m / max;
            let width = (norm * 40.0).round() as usize; // up tp 40 characters wide
            let bar = "█".repeat(width.max(1));
            println!("win {i:>4}: {m:>8.4} micro-second / fact | {bar}");
        }
    }
}

pub async fn run_storage_benchmark() {
    let n_iterations = 10000;
    let seed = 42_u64; // Constant seed for reproducibility

    let mut rng = StdRng::seed_from_u64(seed);
    let mut time_measurement = TimeMeasurement::new();

    let mut storage = MapStorage::new();
    let mut contracts_trie_root_hash = HashOutput::default();
    let mut classes_trie_root_hash = HashOutput::default();

    let contract_num: u128 = (CONTRACT_ADDRESS as u128) + NodeIndex::FIRST_LEAF.0.low();
    let contract_leaf: ContractAddress = contract_num.into();

    for i in 0..n_iterations {
        info!("Committer storage benchmark iteration {}/{}", i + 1, n_iterations);
        let input = InputImpl {
            state_diff: generate_random_state_diff(&mut rng),
            contracts_trie_root_hash,
            classes_trie_root_hash,
            config: ConfigImpl::default(),
        };

        // commit block and write to storage
        time_measurement.start_measurement();
        let serialized_filled_forest = SerializedForest(
            commit_block(input, &mut storage).await.expect("Failed to commit the given block."),
        );
        serialized_filled_forest.0.write_to_storage(&mut storage);

        let n_new_facts =
            serialized_filled_forest.0.storage_tries.get(&contract_leaf).unwrap().tree_map.len();
        time_measurement.stop_measurement(n_new_facts);

        contracts_trie_root_hash = serialized_filled_forest.0.get_contract_root_hash();
        classes_trie_root_hash = serialized_filled_forest.0.get_compiled_class_root_hash();
    }

    time_measurement.pretty_print(50);
}
