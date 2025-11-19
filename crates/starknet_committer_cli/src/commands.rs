use std::path::Path;

use rand::rngs::SmallRng;
use rand::SeedableRng;
use starknet_api::hash::{HashOutput, StateRoots};
use starknet_committer::block_committer::commit::commit_block;
use starknet_committer::block_committer::input::{FactsDbInitialRead, Input, ReaderConfig};
use starknet_committer::block_committer::timing_util::{Action, TimeMeasurement};
use starknet_committer::db::facts_db::db::FactsDb;
use starknet_committer::db::forest_trait::ForestWriter;
use starknet_patricia_storage::aerospike_storage::{AerospikeStorage, AerospikeStorageConfig};
use starknet_patricia_storage::map_storage::{CachedStorage, CachedStorageConfig, MapStorage};
use starknet_patricia_storage::mdbx_storage::MdbxStorage;
use starknet_patricia_storage::rocksdb_storage::{RocksDbOptions, RocksDbStorage};
use starknet_patricia_storage::short_key_storage::ShortKeySize;
use starknet_patricia_storage::storage_trait::{Storage, StorageStats};
use tokio::task::JoinSet;
use tracing::{error, info, warn};

use crate::interference::apply_interference;
use crate::presets::types::flavors::{
    BenchmarkFlavor,
    FlavorFields,
    InterferenceFields,
    InterferenceFlavor,
};
use crate::presets::types::storage::{
    AerospikeFields,
    SingleMemoryStorageFields,
    SingleStorageFields,
    SpecificDbFields,
    StorageLayout,
    StorageLayoutName,
};
use crate::presets::types::PresetFields;

pub type InputImpl = Input<FactsDbInitialRead>;

pub async fn run_benchmark(preset_fields: &PresetFields) {
    match preset_fields.storage_layout() {
        StorageLayout::Fact(single_storage_fields) => match single_storage_fields {
            SingleStorageFields::FileBased(file_storage_fields) => {
                file_storage_fields.initialize_storage_path();
                let storage_path = Path::new(&file_storage_fields.storage_path);
                let cache_fields = file_storage_fields.global_fields.cache_fields.clone();
                match &file_storage_fields.specific_db_fields {
                    SpecificDbFields::RocksDb(rocksdb_fields) => {
                        let rocksdb_options = if rocksdb_fields.allow_mmap {
                            RocksDbOptions::default()
                        } else {
                            RocksDbOptions::default_no_mmap()
                        };
                        let storage = RocksDbStorage::open(
                            storage_path,
                            rocksdb_options,
                            rocksdb_fields.use_column_families,
                        )
                        .unwrap();
                        add_cache_and_run_benchmark(
                            single_storage_fields,
                            preset_fields.flavor_fields(),
                            storage,
                            &cache_fields,
                        )
                        .await;
                    }
                    SpecificDbFields::Mdbx(_mdbx_fields) => {
                        let storage = MdbxStorage::open(storage_path).unwrap();
                        add_cache_and_run_benchmark(
                            single_storage_fields,
                            preset_fields.flavor_fields(),
                            storage,
                            &cache_fields,
                        )
                        .await;
                    }
                    SpecificDbFields::Aerospike(AerospikeFields { aeroset, namespace, hosts }) => {
                        let config = AerospikeStorageConfig::new_default(
                            aeroset.clone(),
                            namespace.clone(),
                            hosts.clone(),
                        );
                        let storage = AerospikeStorage::new(config).await.unwrap();
                        add_cache_and_run_benchmark(
                            single_storage_fields,
                            preset_fields.flavor_fields(),
                            storage,
                            &cache_fields,
                        )
                        .await;
                    }
                }
            }
            SingleStorageFields::Memory(SingleMemoryStorageFields(
                single_memory_storage_fields,
            )) => {
                let storage = MapStorage::default();
                add_cache_and_run_benchmark(
                    single_storage_fields,
                    preset_fields.flavor_fields(),
                    storage,
                    &single_memory_storage_fields.cache_fields,
                )
                .await;
            }
        },
    }
}

async fn add_cache_and_run_benchmark<S: Storage>(
    single_storage_fields: &SingleStorageFields,
    flavor_fields: &FlavorFields,
    storage: S,
    cache_storage_config: &Option<CachedStorageConfig>,
) {
    if let Some(cache_storage_config) = cache_storage_config {
        run_storage_benchmark_wrapper(
            single_storage_fields,
            flavor_fields,
            CachedStorage::new(storage, cache_storage_config.clone()),
        )
        .await;
    } else {
        run_storage_benchmark_wrapper(single_storage_fields, flavor_fields, storage).await;
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
                Some(ShortKeySize::$size) => {
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
    single_storage_fields: &SingleStorageFields,
    FlavorFields {
        seed,
        n_iterations,
        flavor,
        checkpoint_interval,
        n_updates,
        interference_fields,
        data_path,
        ..
    }: &FlavorFields,
    storage: S,
) {
    let key_size = single_storage_fields.global_fields().short_key_size.clone();

    let storage_type_name = single_storage_fields.short_name();
    let output_dir = format!("{data_path}/{storage_type_name}/csvs/{n_iterations}");
    let checkpoint_dir = format!("{data_path}/{storage_type_name}/checkpoints/{n_iterations}");

    generate_short_key_benchmark!(
        key_size,
        *seed,
        *n_iterations,
        flavor.clone(),
        *n_updates,
        interference_fields.clone(),
        output_dir,
        Some(checkpoint_dir.as_str()),
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
#[allow(clippy::too_many_arguments)]
pub async fn run_storage_benchmark<S: Storage>(
    seed: u64,
    n_iterations: usize,
    flavor: BenchmarkFlavor,
    n_updates_arg: usize,
    InterferenceFields { interference_type, interference_concurrency_limit }: InterferenceFields,
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
            initial_read_context: FactsDbInitialRead(StateRoots {
                contracts_trie_root_hash,
                classes_trie_root_hash,
            }),
            config: ReaderConfig::default(),
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
                    &interference_type,
                    &flavor,
                    n_updates_arg,
                    block_number,
                    &mut interference_task_set,
                    async_storage,
                    &mut rng,
                )
                .await;
            } else if !matches!(interference_type, InterferenceFlavor::None) {
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
