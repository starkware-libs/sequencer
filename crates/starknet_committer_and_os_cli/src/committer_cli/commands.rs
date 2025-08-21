use std::collections::HashMap;
use std::time::Instant;

use rand::rngs::StdRng;
use rand::SeedableRng;
use starknet_committer::block_committer::commit::commit_block;
use starknet_committer::block_committer::input::{Config, ConfigImpl};
use starknet_committer::block_committer::state_diff_generator::generate_random_state_diff;
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia_storage::map_storage::{BorrowedMapStorage, MapStorage};
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

pub async fn run_storage_benchmark() {
    let n_iterations = 10;
    let seed = 42_u64; // Constant seed for reproducibility

    let mut rng = StdRng::seed_from_u64(seed);
    let mut total_time = 0;

    let mut storage = MapStorage::new();
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

        // commit block and write to storage
        let start = Instant::now();
        let serialized_filled_forest = SerializedForest(
            commit_block(input, &mut storage).await.expect("Failed to commit the given block."),
        );
        serialized_filled_forest.0.write_to_storage(&mut storage);
        let duration = start.elapsed();
        total_time += duration.as_millis();

        contracts_trie_root_hash = serialized_filled_forest.0.get_contract_root_hash();
        classes_trie_root_hash = serialized_filled_forest.0.get_compiled_class_root_hash();
    }

    let avg_millis = total_time as f64 / n_iterations as f64;
    info!("Average time: {:.2} milliseconds", avg_millis);
}
