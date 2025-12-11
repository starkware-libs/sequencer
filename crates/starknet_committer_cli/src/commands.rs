use blake2::digest::consts::U31;
use blake2::{Blake2s, Digest};
use rand::distributions::Uniform;
use rand::prelude::IteratorRandom;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use starknet_api::hash::{HashOutput, StateRoots};
use starknet_committer::block_committer::commit::commit_block;
use starknet_committer::block_committer::input::{
    ConfigImpl,
    FactsDInitialRead,
    Input,
    StarknetStorageKey,
    StateDiff,
};
use starknet_committer::block_committer::state_diff_generator::generate_random_state_diff;
use starknet_committer::block_committer::timing_util::{Action, TimeMeasurement};
use starknet_committer::db::facts_db::FactsDb;
use starknet_committer::db::forest_trait::ForestWriter;
use starknet_patricia_storage::storage_trait::{AsyncStorage, DbKey, Storage, StorageStats};
use starknet_types_core::felt::Felt;
use tokio::task::JoinSet;
use tracing::{error, info, warn};

use crate::args::{
    BenchmarkFlavor,
    GlobalArgs,
    InterferenceArgs,
    InterferenceType,
    ShortKeySizeArg,
    StorageBenchmarkCommand,
    StorageType,
    DEFAULT_DATA_PATH,
};

pub type InputImpl = Input<ConfigImpl, FactsDInitialRead>;

const FLAVOR_PERIOD_MANY_WINDOW: usize = 10;
const FLAVOR_PERIOD_PERIOD: usize = 500;

const FLAVOR_OVERLAP_WARMUP_BLOCKS: usize = 100_000;

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
            StarknetStorageKey::try_from(Felt::from_bytes_be_slice(result.as_slice())).unwrap()
        })
        .collect()
}

impl BenchmarkFlavor {
    /// Returns the total amount of nonzero leaves in the system up to (not including) the block
    /// number.
    fn total_nonzero_leaves_up_to(&self, n_updates_arg: usize, block_number: usize) -> usize {
        let twenty_percent = n_updates_arg / 5;
        match self {
            Self::Constant | Self::Continuous => block_number * n_updates_arg,
            Self::Overlap => {
                if block_number < FLAVOR_OVERLAP_WARMUP_BLOCKS {
                    block_number * n_updates_arg
                } else {
                    FLAVOR_OVERLAP_WARMUP_BLOCKS * n_updates_arg
                        + (block_number - FLAVOR_OVERLAP_WARMUP_BLOCKS) * twenty_percent
                }
            }
            Self::PeriodicPeaks => {
                let updates_per_period = n_updates_arg * FLAVOR_PERIOD_MANY_WINDOW
                    + twenty_percent * (FLAVOR_PERIOD_PERIOD - FLAVOR_PERIOD_MANY_WINDOW);
                let mod_period = block_number % FLAVOR_PERIOD_PERIOD;
                let is_many_window = mod_period < FLAVOR_PERIOD_MANY_WINDOW;

                let total_leaves_added_in_period = if is_many_window {
                    // We are still in the initial window with many updates.
                    n_updates_arg * mod_period
                } else {
                    // We have passed the many-updates window.
                    n_updates_arg * FLAVOR_PERIOD_MANY_WINDOW
                        + twenty_percent * (mod_period - FLAVOR_PERIOD_MANY_WINDOW)
                };
                (block_number / FLAVOR_PERIOD_PERIOD) * updates_per_period
                    + total_leaves_added_in_period
            }
        }
    }

    /// Returns the keys of the leaves that are updated in the given block.
    /// Depending on the flavor, some of the leaves to be updated are chosen randomly from the
    /// previous leaves, but all new leaf indices are deterministic.
    fn leaf_update_keys(
        &self,
        n_updates_arg: usize,
        block_number: usize,
        rng: &mut SmallRng,
    ) -> Vec<StarknetStorageKey> {
        let twenty_percent = n_updates_arg / 5;
        let total_leaves = self.total_nonzero_leaves_up_to(n_updates_arg, block_number);
        match self {
            Self::Constant => {
                leaf_preimages_to_storage_keys(total_leaves..(total_leaves + n_updates_arg))
            }
            Self::Continuous => (total_leaves..(total_leaves + n_updates_arg))
                .map(|i| StarknetStorageKey::try_from(Felt::from(i)).unwrap())
                .collect(),
            Self::Overlap => {
                // Invariant: if there are a total of L leaves in the DB, then the nonzero keys are
                // [hash(i) for i in 0..L].
                // Warmup phase: all leaves should be new, until 100M nonzero leaves exist.
                leaf_preimages_to_storage_keys(if block_number < FLAVOR_OVERLAP_WARMUP_BLOCKS {
                    // Warmup phase: all leaves should be new.
                    (total_leaves..(total_leaves + n_updates_arg)).collect()
                } else {
                    // We are warmed up, so only 20% of the leaves should be new.
                    // The total number of updates remains constant in this flavor.
                    // Sample (n_updates-new_leaves) old indices uniformly at random, from the
                    // previous leaves. Choose leaves from the (overlap_warmup_blocks * n_updates)
                    // most recent leaves.
                    let start_index = total_leaves - (FLAVOR_OVERLAP_WARMUP_BLOCKS * n_updates_arg);
                    let n_overlap_leaves = n_updates_arg - twenty_percent;
                    let updated_keys =
                        (start_index..total_leaves).choose_multiple(rng, n_overlap_leaves);
                    let new_keys = (total_leaves..(total_leaves + twenty_percent)).collect();
                    [updated_keys, new_keys].concat()
                })
            }
            Self::PeriodicPeaks => {
                let new_leaves = if block_number % FLAVOR_PERIOD_PERIOD < FLAVOR_PERIOD_MANY_WINDOW
                {
                    n_updates_arg
                } else {
                    twenty_percent
                };
                leaf_preimages_to_storage_keys(total_leaves..(total_leaves + new_leaves))
            }
        }
    }

    /// The nonzero leaf indices in the system are uniquely determined by the block number (see
    /// [Self::leaf_update_keys]), however, the actual state diff can be random depending on the
    /// flavor (nonzero leaf updates can be randomized).
    fn generate_state_diff(
        &self,
        n_updates_arg: usize,
        block_number: usize,
        rng: &mut SmallRng,
    ) -> StateDiff {
        let leaf_keys = self.leaf_update_keys(n_updates_arg, block_number, rng);
        let n_updates = leaf_keys.len();
        generate_random_state_diff(rng, n_updates, Some(leaf_keys))
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
        $n_updates:expr,
        $interference_type:expr,
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
                    $n_updates,
                    $interference_type,
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
                        $n_updates,
                        $interference_type,
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
        n_updates,
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
        *flavor,
        *n_updates,
        storage_benchmark_args.interference_args(),
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

fn apply_interference<S: AsyncStorage>(
    interference_type: InterferenceType,
    benchmark_flavor: BenchmarkFlavor,
    n_updates_arg: usize,
    block_number: usize,
    task_set: &mut JoinSet<()>,
    mut storage: S,
    rng: &mut SmallRng,
) {
    match interference_type {
        InterferenceType::None => {}
        InterferenceType::Read1KEveryBlock => {
            let total_leaves =
                benchmark_flavor.total_nonzero_leaves_up_to(n_updates_arg, block_number + 1);
            // Avoid creating an iterator over the entire range - select random leaves, with
            // possible repetition. Probability of repitition will decrease as the number of
            // leaves increases.
            let dist = Uniform::new(0, total_leaves);
            let preimages = (0..INTERFERENCE_READ_1K_EVERY_BLOCK_N_READS)
                .map(|_| rng.sample(dist))
                .collect::<Vec<_>>();
            task_set.spawn(async move {
                let keys = leaf_preimages_to_storage_keys(preimages)
                    .iter()
                    .map(|k| DbKey((**k.0).to_bytes_be().to_vec()))
                    .collect::<Vec<_>>();
                storage.mget(&keys.iter().collect::<Vec<&DbKey>>()).await.unwrap();
            });
        }
    }
}

/// Runs the committer on n_iterations random generated blocks.
/// Prints the time measurement to the console and saves statistics to a CSV file in the given
/// output directory.
#[allow(clippy::too_many_arguments)]
pub async fn run_storage_benchmark<S: Storage>(
    seed: u64,
    n_iterations: usize,
    flavor: BenchmarkFlavor,
    n_updates_arg: usize,
    InterferenceArgs { interference_type, interference_concurrency_limit }: InterferenceArgs,
    output_dir: &str,
    checkpoint_dir: Option<&str>,
    storage: S,
    checkpoint_interval: usize,
) {
    let mut interference_task_set = JoinSet::new();
    let mut time_measurement = TimeMeasurement::new(checkpoint_interval, S::Stats::column_titles());
    let mut contracts_trie_root_hash = match checkpoint_dir {
        Some(checkpoint_dir) => {
            time_measurement.try_load_from_checkpoint(checkpoint_dir).unwrap_or_default()
        }
        None => HashOutput::default(),
    };
    let curr_block_number = time_measurement.block_number;

    let mut classes_trie_root_hash = HashOutput::default();
    let mut facts_db = FactsDb::new(storage);

    for block_number in curr_block_number..n_iterations {
        info!("Committer storage benchmark iteration {}/{}", block_number + 1, n_iterations);
        // Seed is created from block number, to be independent of restarts using checkpoints.
        let mut rng = SmallRng::seed_from_u64(seed + u64::try_from(block_number).unwrap());
        let input = InputImpl {
            state_diff: flavor.generate_state_diff(n_updates_arg, block_number, &mut rng),
            initial_read_context: FactsDInitialRead(StateRoots {
                contracts_trie_root_hash,
                classes_trie_root_hash,
            }),
            config: ConfigImpl::default(),
        };

        time_measurement.start_measurement(Action::EndToEnd);
        let filled_forest = commit_block(input, &mut facts_db, Some(&mut time_measurement))
            .await
            .expect("Failed to commit the given block.");
        time_measurement.start_measurement(Action::Write);
        let n_new_facts = facts_db.write(&filled_forest).await;
        info!("Written {n_new_facts} new facts to storage");
        time_measurement.stop_measurement(None, Action::Write);

        time_measurement.stop_measurement(Some(n_new_facts), Action::EndToEnd);

        // Export to csv in the checkpoint interval and print the statistics of the storage.
        if (block_number + 1) % checkpoint_interval == 0 {
            let storage_stats = facts_db.storage.get_stats();
            facts_db.storage.reset_stats().unwrap();
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

        // If the storage supports interference (is async), apply interference.
        if let Some(async_storage) = facts_db.storage.get_async_self() {
            // First, try joining all completed interference tasks.
            // Log all failed tasks but do not panic - the benchmark is still running.
            while let Some(result) = interference_task_set.try_join_next() {
                if let Err(error) = result {
                    error!("Interference task failed: {error}.");
                }
            }
            // If the limit is not reached, spawn a new interference task.
            if interference_task_set.len() < interference_concurrency_limit {
                apply_interference(
                    interference_type,
                    flavor,
                    n_updates_arg,
                    block_number,
                    &mut interference_task_set,
                    async_storage,
                    &mut rng,
                );
            } else if !matches!(interference_type, InterferenceType::None) {
                warn!(
                    "Interference concurrency limit ({interference_concurrency_limit}) reached. \
                     Skipping interference task."
                );
            }
        }
    }

    // Export to csv in the last iteration.
    if !n_iterations.is_multiple_of(checkpoint_interval) {
        time_measurement.to_csv(
            &format!("{n_iterations}.csv"),
            output_dir,
            facts_db.storage.get_stats().map(|s| Some(s.column_values())).unwrap_or(None),
        );
    }

    time_measurement.pretty_print(50);

    // Gather all interference tasks and wait for them to complete.
    // At this point it is safe (and preferable) to panic if any remaining task fails, as the
    // benchmark is complete.
    info!("Waiting for {} interference tasks to complete.", interference_task_set.len());
    interference_task_set.join_all().await;
    info!("All interference tasks completed.");
}
