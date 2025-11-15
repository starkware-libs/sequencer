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
use starknet_patricia_storage::storage_trait::{DbKey, Storage, StorageStats};
use starknet_types_core::felt::Felt;
use tokio::task::JoinSet;
use tracing::info;

use crate::args::{
    BenchmarkFlavor,
    GlobalArgs,
    InterferenceFlavor,
    ShortKeySizeArg,
    StorageBenchmarkCommand,
    StorageType,
    DEFAULT_DATA_PATH,
};

pub type InputImpl = Input<ConfigImpl>;

const FLAVOR_1K_N_UPDATES: usize = 1000;
const FLAVOR_4K_N_UPDATES: usize = 4000;

const FLAVOR_PERIOD_MANY_WINDOW: usize = 10;
const FLAVOR_PERIOD_MANY_UPDATES: usize = 1000;
const FLAVOR_PERIOD_FEW_UPDATES: usize = 200;
const FLAVOR_PERIOD_PERIOD: usize = 500;

const FLAVOR_OVERLAP_N_UPDATES: usize = 1000;
const FLAVOR_OVERLAP_WARMUP_BLOCKS: usize = 100_000;
const FLAVOR_OVERLAP_NEW_LEAVES_AFTER_WARMUP: usize = FLAVOR_OVERLAP_N_UPDATES / 5;

const INTERFERENCE_READ_1K_EVERY_BLOCK_N_READS: usize = 1000;

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
    /// Returns the total amount of nonzero leaves in the system up to (not including) the block
    /// number.
    fn total_nonzero_leaves_up_to(&self, block_number: usize) -> usize {
        match self {
            Self::Constant1KDiff => block_number * FLAVOR_1K_N_UPDATES,
            Self::Constant4KDiff => block_number * FLAVOR_4K_N_UPDATES,
            Self::Overlap1KDiff => {
                if block_number < FLAVOR_OVERLAP_WARMUP_BLOCKS {
                    block_number * FLAVOR_OVERLAP_N_UPDATES
                } else {
                    FLAVOR_OVERLAP_WARMUP_BLOCKS * FLAVOR_OVERLAP_N_UPDATES
                        + (block_number - FLAVOR_OVERLAP_WARMUP_BLOCKS)
                            * FLAVOR_OVERLAP_NEW_LEAVES_AFTER_WARMUP
                }
            }
            Self::PeriodicPeaks => {
                let updates_per_period = FLAVOR_PERIOD_MANY_UPDATES * FLAVOR_PERIOD_MANY_WINDOW
                    + FLAVOR_PERIOD_FEW_UPDATES
                        * (FLAVOR_PERIOD_PERIOD - FLAVOR_PERIOD_MANY_WINDOW);
                let mod_period = block_number % FLAVOR_PERIOD_PERIOD;
                let is_many_window = mod_period < FLAVOR_PERIOD_MANY_WINDOW;

                let total_leaves_added_in_period = if is_many_window {
                    // We are still in the initial window with many updates.
                    FLAVOR_PERIOD_MANY_UPDATES * mod_period
                } else {
                    // We have passed the many-updates window.
                    FLAVOR_PERIOD_MANY_UPDATES * FLAVOR_PERIOD_MANY_WINDOW
                        + FLAVOR_PERIOD_FEW_UPDATES * (mod_period - FLAVOR_PERIOD_MANY_WINDOW)
                };
                (block_number / FLAVOR_PERIOD_PERIOD) * updates_per_period
                    + total_leaves_added_in_period
            }
        }
    }

    /// Returns the preimages of the leaves that are updated in the given block.
    /// Invariant: if there are a total of L leaves in the DB, then the nonzero keys are
    /// [hash(i) for i in 0..L]. This means that the existing leaf preimages [0, L] are uniquely
    /// determined by the block number.
    /// Depending on the flavor, some of the leaves to be updated are chosen randomly from the
    /// previous leaves, but all new leaf indices are deterministic.
    fn leaf_update_preimages(&self, block_number: usize, rng: &mut SmallRng) -> Vec<usize> {
        let total_leaves = self.total_nonzero_leaves_up_to(block_number);
        match self {
            Self::Constant1KDiff => (total_leaves..(total_leaves + FLAVOR_1K_N_UPDATES)).collect(),
            Self::Constant4KDiff => (total_leaves..(total_leaves + FLAVOR_4K_N_UPDATES)).collect(),
            Self::Overlap1KDiff => {
                // Invariant: if there are a total of L leaves in the DB, then the nonzero keys are
                // [hash(i) for i in 0..L].
                // Warmup phase: all leaves should be new, until 100M nonzero leaves exist.
                if block_number < FLAVOR_OVERLAP_WARMUP_BLOCKS {
                    // Warmup phase: all leaves should be new.

                    (total_leaves..(total_leaves + FLAVOR_OVERLAP_N_UPDATES)).collect()
                } else {
                    // We are warmed up, so only 20% of the leaves should be new.
                    // The total number of updates remains constant in this flavor.
                    // Sample (n_updates-new_leaves) old indices uniformly at random, from the
                    // previous leaves. Choose leaves from the (overlap_warmup_blocks * n_updates)
                    // most recent leaves.
                    let start_index =
                        total_leaves - (FLAVOR_OVERLAP_WARMUP_BLOCKS * FLAVOR_OVERLAP_N_UPDATES);
                    let n_overlap_leaves =
                        FLAVOR_OVERLAP_N_UPDATES - FLAVOR_OVERLAP_NEW_LEAVES_AFTER_WARMUP;
                    let updated_keys =
                        (start_index..total_leaves).choose_multiple(rng, n_overlap_leaves);
                    let new_keys = (total_leaves
                        ..(total_leaves + FLAVOR_OVERLAP_NEW_LEAVES_AFTER_WARMUP))
                        .collect();
                    [updated_keys, new_keys].concat()
                }
            }
            Self::PeriodicPeaks => {
                let new_leaves = if block_number % FLAVOR_PERIOD_PERIOD < FLAVOR_PERIOD_MANY_WINDOW
                {
                    FLAVOR_PERIOD_MANY_UPDATES
                } else {
                    FLAVOR_PERIOD_FEW_UPDATES
                };
                (total_leaves..(total_leaves + new_leaves)).collect()
            }
        }
    }

    /// The nonzero leaf indices in the system are uniquely determined by the block number (see
    /// [Self::leaf_update_preimages]), however, the actual state diff can be random depending on
    /// the flavor (nonzero leaf updates can be randomized).
    fn generate_state_diff(&self, block_number: usize, rng: &mut SmallRng) -> StateDiff {
        let preimages = self.leaf_update_preimages(block_number, rng);
        let n_updates = preimages.len();
        generate_random_state_diff(rng, n_updates, Some(leaf_preimages_to_storage_keys(preimages)))
    }
}

/// Multiplexer to avoid dynamic dispatch.
/// If the key_size is not None, wraps the storage in a key-shrinking storage before running the
/// benchmark.
macro_rules! generate_short_key_benchmark {
    (
        $global_args:expr,
        $output_dir:expr,
        $checkpoint_dir_arg:expr,
        $storage:expr,
        $( ($size:ident, $name:ident) ),+ $(,)?
    ) => {
        match $global_args.key_size {
            None => {
                run_storage_benchmark($global_args, &$output_dir, $checkpoint_dir_arg, $storage)
                    .await
            }
            $(
                Some(ShortKeySizeArg::$size) => {
                    let storage = starknet_patricia_storage::short_key_storage::$name::new($storage);
                    run_storage_benchmark($global_args, &$output_dir, $checkpoint_dir_arg, storage)
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
    let GlobalArgs { n_iterations, output_dir, checkpoint_dir, .. } =
        storage_benchmark_args.global_args();

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
        storage_benchmark_args.global_args(),
        output_dir,
        checkpoint_dir_arg,
        storage,
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
    GlobalArgs {
        seed,
        n_iterations,
        flavor,
        interference_flavor,
        checkpoint_interval,
        ..
    }: &GlobalArgs,
    output_dir: &str,
    checkpoint_dir: Option<&str>,
    mut storage: S,
) {
    let mut interference_tasks = JoinSet::new();
    let mut time_measurement =
        TimeMeasurement::new(*checkpoint_interval, S::Stats::column_titles());
    let mut contracts_trie_root_hash = match checkpoint_dir {
        Some(checkpoint_dir) => {
            time_measurement.try_load_from_checkpoint(checkpoint_dir).unwrap_or_default()
        }
        None => HashOutput::default(),
    };
    let curr_block_number = time_measurement.block_number;

    let mut classes_trie_root_hash = HashOutput::default();

    for block_number in curr_block_number..*n_iterations {
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

        // Add interference tasks if needed.
        match interference_flavor {
            InterferenceFlavor::None => {}
            InterferenceFlavor::Read1KEveryBlock => {
                let total_leaves = flavor.total_nonzero_leaves_up_to(block_number + 1);
                let mut cloned_storage = storage.clone();
                interference_tasks.spawn(async move {
                    let keys = leaf_preimages_to_storage_keys(
                        (0..total_leaves)
                            .choose_multiple(&mut rng, INTERFERENCE_READ_1K_EVERY_BLOCK_N_READS),
                    )
                    .iter()
                    .map(|k| DbKey((**k.0).to_bytes_be().to_vec()))
                    .collect::<Vec<_>>();
                    cloned_storage.mget(&keys.iter().collect::<Vec<&DbKey>>()).unwrap();
                });
            }
        }
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

    info!("Joining {} interference tasks...", interference_tasks.len());
    interference_tasks.join_all().await;
    info!("Interference tasks joined.");
}
