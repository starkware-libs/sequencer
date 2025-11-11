use std::fs;
use std::num::NonZeroUsize;
use std::path::Path;

use apollo_infra_utils::tracing_utils::{configure_tracing, modify_log_level};
use clap::{Parser, Subcommand};
use starknet_committer_cli::args::{StorageArgs, StorageType};
use starknet_committer_cli::commands::run_storage_benchmark_wrapper;
use starknet_patricia_storage::aerospike_storage::{AerospikeStorage, AerospikeStorageConfig};
use starknet_patricia_storage::map_storage::{CachedStorage, CachedStorageConfig, MapStorage};
use starknet_patricia_storage::mdbx_storage::MdbxStorage;
use starknet_patricia_storage::rocksdb_storage::{RocksDbOptions, RocksDbStorage};
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::reload::Handle;
use tracing_subscriber::Registry;

#[derive(Parser, Debug)]
pub struct CommitterCliCommand {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    StorageBenchmark(StorageArgs),
}

pub async fn run_committer_cli(
    committer_command: CommitterCliCommand,
    log_filter_handle: Handle<LevelFilter, Registry>,
) {
    info!("Starting committer-cli with command: \n{:?}", committer_command);
    match committer_command.command {
        Command::StorageBenchmark(storage_args) => {
            let StorageArgs {
                ref log_level,
                ref storage_path,
                ref data_path,
                ref storage_type,
                ref cache_size,
                allow_mmap,
                include_inner_stats,
                ref aeroset,
                ref namespace,
                ref hosts,
                ..
            } = storage_args;

            modify_log_level(log_level.clone(), log_filter_handle);

            // Construct the storage path.
            // Create the path on filesystem only if we are using filesystem-based storage.
            let storage_path = storage_path
                .clone()
                .unwrap_or_else(|| format!("{data_path}/storage/{storage_type:?}"));
            match storage_type {
                StorageType::MapStorage | StorageType::Aerospike | StorageType::CachedAerospike => {
                }
                StorageType::Mdbx
                | StorageType::CachedMdbx
                | StorageType::Rocksdb
                | StorageType::CachedRocksdb => {
                    fs::create_dir_all(&storage_path).expect("Failed to create storage directory.")
                }
            };

            // Run the storage benchmark.
            // Explicitly create a different concrete storage type in each match arm to avoid
            // dynamic dispatch.
            let cached_storage_config = CachedStorageConfig {
                cache_size: NonZeroUsize::new(*cache_size).unwrap(),
                cache_on_write: true,
                include_inner_stats,
            };
            let rocksdb_options = if allow_mmap {
                RocksDbOptions::default()
            } else {
                RocksDbOptions::default_no_mmap()
            };
            match storage_type {
                StorageType::MapStorage => {
                    let storage = MapStorage::default();
                    run_storage_benchmark_wrapper(storage_args, storage).await;
                }
                StorageType::Mdbx => {
                    let storage = MdbxStorage::open(Path::new(&storage_path)).unwrap();
                    run_storage_benchmark_wrapper(storage_args, storage).await;
                }
                StorageType::CachedMdbx => {
                    let storage = CachedStorage::new(
                        MdbxStorage::open(Path::new(&storage_path)).unwrap(),
                        cached_storage_config,
                    );
                    run_storage_benchmark_wrapper(storage_args, storage).await;
                }
                StorageType::Rocksdb => {
                    let storage =
                        RocksDbStorage::open(Path::new(&storage_path), rocksdb_options).unwrap();
                    run_storage_benchmark_wrapper(storage_args, storage).await;
                }
                StorageType::CachedRocksdb => {
                    let storage =
                        RocksDbStorage::open(Path::new(&storage_path), rocksdb_options).unwrap();
                    let storage = CachedStorage::new(storage, cached_storage_config);
                    run_storage_benchmark_wrapper(storage_args, storage).await;
                }
                StorageType::Aerospike => {
                    let aerospike_storage_config = AerospikeStorageConfig::new_default(
                        aeroset.clone().unwrap(),
                        namespace.clone().unwrap(),
                        hosts.clone().unwrap(),
                    );
                    let storage = AerospikeStorage::new(aerospike_storage_config).unwrap();
                    run_storage_benchmark_wrapper(storage_args, storage).await;
                }
                StorageType::CachedAerospike => {
                    let aerospike_storage_config = AerospikeStorageConfig::new_default(
                        aeroset.clone().unwrap(),
                        namespace.clone().unwrap(),
                        hosts.clone().unwrap(),
                    );
                    let storage = AerospikeStorage::new(aerospike_storage_config).unwrap();
                    let storage = CachedStorage::new(storage, cached_storage_config);
                    run_storage_benchmark_wrapper(storage_args, storage).await;
                }
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let log_filter_handle = configure_tracing();
    let committer_command = CommitterCliCommand::parse();
    run_committer_cli(committer_command, log_filter_handle).await;
}
