use blake2::digest::consts::U31;
use blake2::{Blake2s, Digest};
use rand::prelude::IteratorRandom;
use rand::rngs::SmallRng;
use rand::SeedableRng;
use starknet_api::core::PatriciaKey;
use starknet_api::hash::HashOutput;
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
use starknet_patricia_storage::storage_trait::{Storage, StorageStats};
use starknet_types_core::felt::Felt;
use tracing::info;

use crate::args::{
    BenchmarkFlavor,
    GlobalArgs,
    ShortKeySizeArg,
    StorageBenchmarkCommand,
    StorageType,
    DEFAULT_DATA_PATH,
};

pub type InputImpl = Input<ConfigImpl>;

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
                // Warmup phase: all leaves should be new, until 100M nonzero leaves exist.
                let overlap_warmup_blocks = 100_000;
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

/// Multiplexer to avoid dynamic dispatch.
/// If the key_size is not None, wraps the storage in a key-shrinking storage before running the
/// benchmark.
macro_rules! generate_short_key_benchmark {
    (
        $key_size:expr,
        $seed:expr,
        $n_iterations:expr,
        $flavor:expr,
        $output_dir:expr,
        $checkpoint_dir_arg:expr,
        $storage:expr,
        $checkpoint_interval:expr,
        $( ($size:ident, $name:ident) ),+ $(,)?
    ) => {
        match $key_size {
            None => {
                run_storage_benchmark(
                    $seed,
                    $n_iterations,
                    $flavor,
                    &$output_dir,
                    $checkpoint_dir_arg,
                    $storage,
                    $checkpoint_interval,
                )
                .await
            }
            $(
                Some(ShortKeySizeArg::$size) => {
                    let storage = starknet_patricia_storage::short_key_storage::$name::new($storage);
                    run_storage_benchmark(
                        $seed,
                        $n_iterations,
                        $flavor,
                        &$output_dir,
                        $checkpoint_dir_arg,
                        storage,
                        $checkpoint_interval,
                    )
                    .await
                }
            )+
        }
    }
}

/// Wrapper to reduce boilerplate and avoid having to use `Box<dyn Storage>`.
/// Different invocations of this function are used with different concrete storage types.
pub async fn run_storage_benchmark_wrapper<S: Storage>(
    storage_benchmark_args: &StorageBenchmarkCommand,
    storage: S,
) {
    let GlobalArgs {
        seed,
        n_iterations,
        flavor,
        checkpoint_interval,
        output_dir,
        checkpoint_dir,
        key_size,
        ..
    } = storage_benchmark_args.global_args();

    let data_path = storage_benchmark_args
        .file_storage_args()
        .map(|file_args| file_args.data_path.clone())
        .unwrap_or(DEFAULT_DATA_PATH.to_string());
    let storage_type = storage_benchmark_args.storage_type();
    let output_dir = output_dir
        .clone()
        .unwrap_or_else(|| format!("{data_path}/{storage_type:?}/csvs/{n_iterations}"));
    let checkpoint_dir = checkpoint_dir
        .clone()
        .unwrap_or_else(|| format!("{data_path}/{storage_type:?}/checkpoints/{n_iterations}"));

    let checkpoint_dir_arg = match storage_type {
        StorageType::Mdbx
        | StorageType::CachedMdbx
        | StorageType::Rocksdb
        | StorageType::CachedRocksdb
        | StorageType::Aerospike
        | StorageType::CachedAerospike => Some(checkpoint_dir.as_str()),
        StorageType::MapStorage | StorageType::CachedMapStorage => None,
    };

    generate_short_key_benchmark!(
        key_size,
        *seed,
        *n_iterations,
        flavor.clone(),
        output_dir,
        checkpoint_dir_arg,
        storage,
        *checkpoint_interval,
        (U16, ShortKeyStorage16),
        (U17, ShortKeyStorage17),
        (U18, ShortKeyStorage18),
        (U19, ShortKeyStorage19),
        (U20, ShortKeyStorage20),
        (U21, ShortKeyStorage21),
        (U22, ShortKeyStorage22),
        (U23, ShortKeyStorage23),
        (U24, ShortKeyStorage24),
        (U25, ShortKeyStorage25),
        (U26, ShortKeyStorage26),
        (U27, ShortKeyStorage27),
        (U28, ShortKeyStorage28),
        (U29, ShortKeyStorage29),
        (U30, ShortKeyStorage30),
        (U31, ShortKeyStorage31),
        (U32, ShortKeyStorage32)
    );
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
    let mut time_measurement = TimeMeasurement::new(checkpoint_interval, S::Stats::column_titles());
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
        let n_new_facts = filled_forest.write_to_storage(&mut storage);
        info!("Written {n_new_facts} new facts to storage");
        time_measurement.stop_measurement(None, Action::Write);

        time_measurement.stop_measurement(Some(n_new_facts), Action::EndToEnd);

        // Export to csv in the checkpoint interval and print the statistics of the storage.
        if (block_number + 1) % checkpoint_interval == 0 {
            let storage_stats = storage.get_stats();
            storage.reset_stats().unwrap();
            time_measurement.to_csv(
                &format!("{}.csv", block_number + 1),
                output_dir,
                storage_stats.as_ref().map(|s| Some(s.column_values())).unwrap_or(None),
            );
            if let Some(checkpoint_dir) = checkpoint_dir {
                time_measurement.save_checkpoint(
                    checkpoint_dir,
                    block_number + 1,
                    &contracts_trie_root_hash,
                )
            }
            info!(
                "{}",
                storage_stats
                    .map(|s| format!("{s}"))
                    .unwrap_or_else(|e| format!("Failed to retrieve statistics: {e}"))
            );
        }
        contracts_trie_root_hash = filled_forest.get_contract_root_hash();
        classes_trie_root_hash = filled_forest.get_compiled_class_root_hash();
    }

    // Export to csv in the last iteration.
    if n_iterations % checkpoint_interval != 0 {
        time_measurement.to_csv(
            &format!("{n_iterations}.csv"),
            output_dir,
            storage.get_stats().map(|s| Some(s.column_values())).unwrap_or(None),
        );
    }

    time_measurement.pretty_print(50);
}
