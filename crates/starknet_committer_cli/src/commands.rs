use rand::rngs::SmallRng;
use rand::SeedableRng;
use starknet_committer::block_committer::commit::commit_block;
use starknet_committer::block_committer::input::{ConfigImpl, Input};
use starknet_committer::block_committer::state_diff_generator::generate_random_state_diff;
use starknet_committer::block_committer::timing_util::{Action, TimeMeasurement};
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia_storage::map_storage::MapStorage;
use tracing::info;

pub type InputImpl = Input<ConfigImpl>;

/// Runs the committer on n_iterations random generated blocks.
/// Prints the time measurement to the console and saves statistics to a CSV file in the given
/// output directory.
pub async fn run_storage_benchmark(seed: u64, n_iterations: usize, output_dir: &str) {
    let mut rng = SmallRng::seed_from_u64(seed);
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

        time_measurement.start_measurement(Action::EndToEnd);
        let filled_forest = commit_block(input, &mut storage, Some(&mut time_measurement))
            .await
            .expect("Failed to commit the given block.");
        time_measurement.start_measurement(Action::Write);
        let n_new_facts = filled_forest.write_to_storage(&mut storage);
        time_measurement.stop_measurement(None, Action::Write);

        time_measurement.stop_measurement(Some(n_new_facts), Action::EndToEnd);

        contracts_trie_root_hash = filled_forest.get_contract_root_hash();
        classes_trie_root_hash = filled_forest.get_compiled_class_root_hash();
    }

    time_measurement.pretty_print(50);
    time_measurement.to_csv(&format!("{n_iterations}.csv"), output_dir);
}
