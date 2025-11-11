use apollo_infra_utils::tracing_utils::{configure_tracing, modify_log_level};
use clap::{Parser, Subcommand};
use starknet_committer_cli::args::{GlobalArgs, StorageBenchmarkCommand, StorageFromArgs};
use starknet_committer_cli::commands::run_storage_benchmark_wrapper;
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
    #[clap(subcommand)]
    StorageBenchmark(StorageBenchmarkCommand),
}

pub async fn run_committer_cli(
    committer_command: CommitterCliCommand,
    log_filter_handle: Handle<LevelFilter, Registry>,
) {
    info!("Starting committer-cli with command: \n{:?}", committer_command);
    match committer_command.command {
        Command::StorageBenchmark(storage_benchmark_args) => {
            let GlobalArgs { ref log_level, .. } = storage_benchmark_args.global_args();
            modify_log_level(log_level.clone(), log_filter_handle);

            // Run the storage benchmark.
            // Explicitly create a different concrete storage type in each match arm to avoid
            // dynamic dispatch.
            match storage_benchmark_args {
                StorageBenchmarkCommand::Memory(ref memory_args) => {
                    let storage = memory_args.storage();
                    run_storage_benchmark_wrapper(&storage_benchmark_args, storage).await;
                }
                StorageBenchmarkCommand::CachedMemory(ref cached_memory_args) => {
                    let storage = cached_memory_args.storage();
                    run_storage_benchmark_wrapper(&storage_benchmark_args, storage).await;
                }
                StorageBenchmarkCommand::Mdbx(ref mdbx_args) => {
                    let storage = mdbx_args.storage();
                    run_storage_benchmark_wrapper(&storage_benchmark_args, storage).await;
                }
                StorageBenchmarkCommand::CachedMdbx(ref cached_mdbx_args) => {
                    let storage = cached_mdbx_args.storage();
                    run_storage_benchmark_wrapper(&storage_benchmark_args, storage).await;
                }
                StorageBenchmarkCommand::Rocksdb(ref rocksdb_args) => {
                    let storage = rocksdb_args.storage();
                    run_storage_benchmark_wrapper(&storage_benchmark_args, storage).await;
                }
                StorageBenchmarkCommand::CachedRocksdb(ref cached_rocksdb_args) => {
                    let storage = cached_rocksdb_args.storage();
                    run_storage_benchmark_wrapper(&storage_benchmark_args, storage).await;
                }
                StorageBenchmarkCommand::Aerospike(ref aerospike_args) => {
                    let storage = aerospike_args.storage();
                    run_storage_benchmark_wrapper(&storage_benchmark_args, storage).await;
                }
                StorageBenchmarkCommand::CachedAerospike(ref cached_aerospike_args) => {
                    let storage = cached_aerospike_args.storage();
                    run_storage_benchmark_wrapper(&storage_benchmark_args, storage).await;
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
