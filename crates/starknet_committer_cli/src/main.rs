use std::fs;
use std::num::NonZeroUsize;
use std::path::Path;

use apollo_infra_utils::tracing_utils::{configure_tracing, modify_log_level};
use clap::{Args, Parser, Subcommand};
use starknet_committer_cli::commands::run_storage_benchmark;
use starknet_patricia_storage::map_storage::{CachedStorage, MapStorage};
use starknet_patricia_storage::mdbx_storage::MdbxStorage;
use starknet_patricia_storage::storage_trait::Storage;
use tracing::info;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::reload::Handle;
use tracing_subscriber::Registry;

#[derive(Parser, Debug)]
pub struct CommitterCliCommand {
    #[clap(subcommand)]
    command: Command,
}

#[derive(clap::ValueEnum, Clone, PartialEq, Debug)]
pub enum StorageType {
    MapStorage,
    Mdbx,
    CachedMdbx,
}

const DEFAULT_DATA_PATH: &str = "/tmp/committer_storage_benchmark";

#[derive(Debug, Args)]
struct StorageArgs {
    /// Seed for the random number generator.
    #[clap(short = 's', long, default_value = "42")]
    seed: u64,
    /// Number of iterations to run the benchmark.
    #[clap(long, default_value = "1000")]
    n_iterations: usize,
    /// Number of storage updates per iteration.
    #[clap(long, default_value = "1000")]
    n_diffs: usize,
    /// Storage impl to use. Note that MapStorage isn't persisted in the file system, so
    /// checkpointing is ignored.
    #[clap(long, default_value = "cached-mdbx")]
    storage_type: StorageType,
    /// If not none, wraps the storage in a key-shrinking storage.
    /// See `short_key_storage.rs` for more details and allowed values.
    #[clap(long, default_value = None)]
    key_size: Option<u8>,
    /// If using cached storage, the size of the cache.
    #[clap(long, default_value = "1000000")]
    cache_size: usize,
    #[clap(long, default_value = "1000")]
    checkpoint_interval: usize,
    #[clap(long, default_value = "warn")]
    log_level: String,
    /// A path to a directory to store the DB, output and checkpoints unless they are
    /// explicitly provided. Defaults to "/tmp/committer_storage_benchmark/".
    #[clap(short = 'd', long, default_value = DEFAULT_DATA_PATH)]
    data_path: String,
    /// A path to a directory to store the DB if needed.
    #[clap(long, default_value = None)]
    storage_path: Option<String>,
    /// A path to a directory to store the csv outputs. If not given, creates a dir according to
    /// the  n_iterations (i.e., rwo runs with different n_iterations will have different csv
    /// outputs)
    #[clap(long, default_value = None)]
    output_dir: Option<String>,
    /// A path to a directory to store the checkpoints to allow benchmark recovery. If not given,
    /// creates a dir according to the n_iterations (i.e., two runs with different n_iterations
    /// will have different checkpoints)
    #[clap(long, default_value = None)]
    checkpoint_dir: Option<String>,
}

#[derive(Debug, Subcommand)]
enum Command {
    StorageBenchmark(StorageArgs),
}

/// Multiplexer to avoid dynamic dispatch.
/// If the key_size is not None, wraps the storage in a key-shrinking storage before running the
/// benchmark.
macro_rules! generate_short_key_benchmark {
    (
        $key_size:expr,
        $seed:expr,
        $n_iterations:expr,
        $n_diffs:expr,
        $output_dir:expr,
        $checkpoint_dir_arg:expr,
        $storage:expr,
        $checkpoint_interval:expr,
        $( ($size:expr, $name:ident) ),+ $(,)?
    ) => {
        match $key_size {
            None => {
                run_storage_benchmark(
                    $seed,
                    $n_iterations,
                    $n_diffs,
                    &$output_dir,
                    $checkpoint_dir_arg,
                    $storage,
                    $checkpoint_interval,
                )
                .await
            }
            $(
                Some(size) if size == $size => {
                    let storage = starknet_patricia_storage::short_key_storage::$name::new($storage);
                    run_storage_benchmark(
                        $seed,
                        $n_iterations,
                        $n_diffs,
                        &$output_dir,
                        $checkpoint_dir_arg,
                        storage,
                        $checkpoint_interval,
                    )
                    .await
                }
            )+
            Some(other_size) => panic!("Invalid key size: {other_size}"),
        }
    }
}

/// Wrapper to reduce boilerplate and avoid having to use `Box<dyn Storage>`.
/// Different invocations of this function are used with different concrete storage types.
async fn run_storage_benchmark_wrapper<S: Storage>(
    StorageArgs {
        seed,
        n_iterations,
        n_diffs,
        storage_type,
        checkpoint_interval,
        data_path,
        output_dir,
        checkpoint_dir,
        key_size,
        ..
    }: StorageArgs,
    storage: S,
) {
    let output_dir = output_dir
        .clone()
        .unwrap_or_else(|| format!("{data_path}/{storage_type:?}/csvs/{n_iterations}"));
    let checkpoint_dir = checkpoint_dir
        .clone()
        .unwrap_or_else(|| format!("{data_path}/{storage_type:?}/checkpoints/{n_iterations}"));

    let checkpoint_dir_arg = match storage_type {
        StorageType::Mdbx | StorageType::CachedMdbx => Some(checkpoint_dir.as_str()),
        StorageType::MapStorage => None,
    };

    generate_short_key_benchmark!(
        key_size,
        seed,
        n_iterations,
        n_diffs,
        output_dir,
        checkpoint_dir_arg,
        storage,
        checkpoint_interval,
        (16, ShortKeyStorage16),
        (17, ShortKeyStorage17),
        (18, ShortKeyStorage18),
        (19, ShortKeyStorage19),
        (20, ShortKeyStorage20),
        (21, ShortKeyStorage21),
        (22, ShortKeyStorage22),
        (23, ShortKeyStorage23),
        (24, ShortKeyStorage24),
        (25, ShortKeyStorage25),
        (26, ShortKeyStorage26),
        (27, ShortKeyStorage27),
        (28, ShortKeyStorage28),
        (29, ShortKeyStorage29),
        (30, ShortKeyStorage30),
        (31, ShortKeyStorage31),
        (32, ShortKeyStorage32)
    );
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
                ..
            } = storage_args;

            modify_log_level(log_level.clone(), log_filter_handle);

            // Construct the storage path.
            // Create the path on filesystem only if we are using filesystem-based storage.
            let storage_path = storage_path
                .clone()
                .unwrap_or_else(|| format!("{data_path}/storage/{storage_type:?}"));
            match storage_type {
                StorageType::MapStorage => (),
                StorageType::Mdbx | StorageType::CachedMdbx => {
                    fs::create_dir_all(&storage_path).expect("Failed to create storage directory.")
                }
            };

            // Run the storage benchmark.
            // Explicitly create a different concrete storage type in each match arm to avoid
            // dynamic dispatch.
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
                        NonZeroUsize::new(*cache_size).unwrap(),
                    );
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
