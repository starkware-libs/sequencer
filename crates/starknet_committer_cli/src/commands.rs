use rand::rngs::SmallRng;
use rand::SeedableRng;
use starknet_committer::block_committer::commit::commit_block;
use starknet_committer::block_committer::input::{ConfigImpl, Input};
use starknet_committer::block_committer::state_diff_generator::generate_random_state_diff;
use starknet_committer::block_committer::timing_util::{Action, TimeMeasurement};
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia_storage::storage_trait::Storage;
use tracing::info;

pub type InputImpl = Input<ConfigImpl>;

/// Runs the committer on n_iterations random generated blocks.
/// Prints the time measurement to the console and saves statistics to a CSV file in the given
/// output directory.
pub async fn run_storage_benchmark<S: Storage>(
    seed: u64,
    n_iterations: usize,
    n_storage_updates_per_iteration: usize,
    output_dir: &str,
    checkpoint_dir: Option<&str>,
    mut storage: S,
    checkpoint_interval: usize,
) {
    let mut time_measurement = TimeMeasurement::new(checkpoint_interval);
    let mut contracts_trie_root_hash = match checkpoint_dir {
        Some(checkpoint_dir) => {
            time_measurement.try_load_from_checkpoint(checkpoint_dir).unwrap_or_default()
        }
        None => HashOutput::default(),
    };
    let curr_block_number = time_measurement.block_number;

    let mut classes_trie_root_hash = HashOutput::default();

    for i in curr_block_number..n_iterations {
        info!("Committer storage benchmark iteration {}/{}", i + 1, n_iterations);
        // Seed is created from block number, to be independent of restarts using checkpoints.
        let mut rng = SmallRng::seed_from_u64(seed + u64::try_from(i).unwrap());
        let input = InputImpl {
            state_diff: generate_random_state_diff(&mut rng, n_storage_updates_per_iteration),
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
        info!("Written {n_new_facts} new facts to storage");
        time_measurement.stop_measurement(None, Action::Write);

        time_measurement.stop_measurement(Some(n_new_facts), Action::EndToEnd);

        // Export to csv in the checkpoint interval and print the statistics of the storage.
        if (i + 1) % checkpoint_interval == 0 {
            time_measurement.to_csv(&format!("{}.csv", i + 1), output_dir);
            if let Some(checkpoint_dir) = checkpoint_dir {
                time_measurement.save_checkpoint(checkpoint_dir, i + 1, &contracts_trie_root_hash)
            }
            storage.print_stats();
        }
        contracts_trie_root_hash = filled_forest.get_contract_root_hash();
        classes_trie_root_hash = filled_forest.get_compiled_class_root_hash();
    }

    // Export to csv in the last iteration.
    if n_iterations % checkpoint_interval != 0 {
        time_measurement.to_csv(&format!("{n_iterations}.csv"), output_dir);
    }

    time_measurement.pretty_print(50);
}
