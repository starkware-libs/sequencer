use blake2::digest::consts::U31;
use blake2::{Blake2s, Digest};
use rand::prelude::IteratorRandom;
use rand::rngs::SmallRng;
use rand::SeedableRng;
use starknet_api::core::PatriciaKey;
use starknet_api::state::StorageKey;
use starknet_committer::block_committer::commit::commit_block;
use starknet_committer::block_committer::input::{
    ConfigImpl,
    Input,
    StarknetStorageKey,
    StateDiff,
};
use starknet_committer::block_committer::state_diff_generator::generate_random_state_diff;
use starknet_committer::block_committer::timing_util::{Action, TimeMeasurement};
use starknet_patricia::hash::hash_trait::HashOutput;
use starknet_patricia::patricia_merkle_tree::filled_tree::node_serde::PatriciaStorageLayout;
use starknet_patricia_storage::storage_trait::Storage;
use starknet_types_core::felt::Felt;
use tracing::info;

pub type InputImpl = Input<ConfigImpl>;

#[derive(clap::ValueEnum, Clone, PartialEq, Debug)]
pub enum BenchmarkFlavor {
    /// Constant 1000 state diffs per iteration.
    #[value(alias("1k-diff"))]
    Constant1KDiff,
    /// Constant 4000 state diffs per iteration.
    #[value(alias("4k-diff"))]
    Constant4KDiff,
    /// Periodic peaks of 1000 state diffs per iteration, with 200 diffs on non-peak iterations.
    /// Peaks are 10 iterations every 500 iterations.
    #[value(alias("peaks"))]
    PeriodicPeaks,
    /// Constant number of state diffs per iteration, with 20% new leaves per iteration. The other
    /// 80% leaf updates are sampled randomly from recent leaf updates.
    /// For the first blocks, behaves just like [Self::Constant1KDiff] ("warmup" phase).
    #[value(alias("overlap-1k-diff"))]
    Overlap1KDiff,
}

/// Given a range, generates pseudorandom 31-byte storage keys hashed from the numbers in the range.
fn leaf_preimages_to_storage_keys(
    indices: impl IntoIterator<Item = usize>,
) -> Vec<StarknetStorageKey> {
    indices
        .into_iter()
        .map(|i| {
            let mut hasher = Blake2s::<U31>::new();
            hasher.update(i.to_be_bytes().as_slice());
            let result = hasher.finalize();
            let key = PatriciaKey::try_from(Felt::from_bytes_be_slice(result.as_slice())).unwrap();
            StarknetStorageKey(StorageKey(key))
        })
        .collect()
}

impl BenchmarkFlavor {
    fn n_updates(&self, iteration: usize) -> usize {
        match self {
            Self::Constant1KDiff | Self::Overlap1KDiff => 1000,
            Self::Constant4KDiff => 4000,
            Self::PeriodicPeaks => {
                if iteration % 500 < 10 {
                    1000
                } else {
                    200
                }
            }
        }
    }

    fn generate_state_diff(&self, block_number: usize, rng: &mut SmallRng) -> StateDiff {
        let n_updates = self.n_updates(block_number);
        let keys_override = match self {
            Self::Constant1KDiff | Self::Constant4KDiff | Self::PeriodicPeaks => None,
            Self::Overlap1KDiff => {
                // Invariant: if there are a total of L leaves in the DB, then the nonzero keys are
                // [hash(i) for i in 0..L].
                let overlap_warmup_blocks = 1000;
                if block_number < overlap_warmup_blocks {
                    // Warmup phase: all leaves should be new.
                    let total_leaves = block_number * n_updates;
                    Some(leaf_preimages_to_storage_keys(total_leaves..total_leaves + n_updates))
                } else {
                    // We are warmed up, so only 20% of the leaves should be new.
                    // The total number of updates remains constant in this flavor.
                    let new_leaves = n_updates / 5;
                    // After warmup, each iteration (block) adds only 20% new leaves.
                    let total_leaves = overlap_warmup_blocks * n_updates
                        + (block_number - overlap_warmup_blocks) * new_leaves;
                    // Sample (n_updates-new_leaves) old indices uniformly at random, from the
                    // previous leaves. Choose leaves from the (overlap_warmup_blocks * n_updates)
                    // most recent leaves.
                    let start_index = total_leaves - (overlap_warmup_blocks * n_updates);
                    let n_overlap_leaves = n_updates - new_leaves;
                    let updated_keys = leaf_preimages_to_storage_keys(
                        (start_index..total_leaves).choose_multiple(rng, n_overlap_leaves),
                    );
                    let new_keys =
                        leaf_preimages_to_storage_keys(total_leaves..total_leaves + new_leaves);
                    Some([updated_keys, new_keys].concat())
                }
            }
        };
        generate_random_state_diff(rng, n_updates, keys_override)
    }
}

/// Runs the committer on n_iterations random generated blocks.
/// Prints the time measurement to the console and saves statistics to a CSV file in the given
/// output directory.
pub async fn run_storage_benchmark<S: Storage>(
    seed: u64,
    n_iterations: usize,
    flavor: BenchmarkFlavor,
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

    for block_number in curr_block_number..n_iterations {
        info!("Committer storage benchmark iteration {}/{}", block_number + 1, n_iterations);
        // Seed is created from block number, to be independent of restarts using checkpoints.
        let mut rng = SmallRng::seed_from_u64(seed + u64::try_from(block_number).unwrap());
        let input = InputImpl {
            state_diff: flavor.generate_state_diff(block_number, &mut rng),
            contracts_trie_root_hash,
            classes_trie_root_hash,
            config: ConfigImpl::default(),
        };

        time_measurement.start_measurement(Action::EndToEnd);
        let filled_forest = commit_block(input, &mut storage, Some(&mut time_measurement))
            .await
            .expect("Failed to commit the given block.");
        time_measurement.start_measurement(Action::Write);
        // TODO(Dori): Get storage layout from input.
        let n_new_facts = filled_forest.write_to_storage(&mut storage, PatriciaStorageLayout::Fact);
        info!("Written {n_new_facts} new facts to storage");
        time_measurement.stop_measurement(None, Action::Write);

        time_measurement.stop_measurement(Some(n_new_facts), Action::EndToEnd);

        // Export to csv in the checkpoint interval and print the statistics of the storage.
        if (block_number + 1) % checkpoint_interval == 0 {
            time_measurement.to_csv(&format!("{}.csv", block_number + 1), output_dir);
            if let Some(checkpoint_dir) = checkpoint_dir {
                time_measurement.save_checkpoint(
                    checkpoint_dir,
                    block_number + 1,
                    &contracts_trie_root_hash,
                )
            }
            if let Some(stats) = storage.get_stats() {
                info!("{}", stats);
            }
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
