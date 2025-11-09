use std::fs;
use std::num::NonZeroUsize;
use std::path::Path;

use apollo_infra_utils::tracing_utils::{configure_tracing, modify_log_level};
use clap::{ArgAction, Args, Parser, Subcommand};
use starknet_committer_cli::commands::{run_storage_benchmark, BenchmarkFlavor};
use starknet_patricia::patricia_merkle_tree::filled_tree::node_serde::PatriciaStorageLayout;
use starknet_patricia_storage::map_storage::{CachedStorage, CachedStorageConfig, MapStorage};
use starknet_patricia_storage::mdbx_storage::MdbxStorage;
use starknet_patricia_storage::rocksdb_storage::{RocksDbOptions, RocksDbStorage};
use starknet_patricia_storage::short_key_storage::ShortKeySize;
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
    Rocksdb,
    CachedRocksdb,
}

const DEFAULT_DATA_PATH: &str = "/tmp/committer_storage_benchmark";

/// Given an enum declaration X with no data in the variants, and another enum Y, add
/// implementations of `From<X> for Y` and `From<Y> for X`.
/// Used to enforce a match between the enum X and the arg-enum Y, when we cannot derive ValueEnum
/// for X directly.
macro_rules! clone_enum_as_arg_enum {
    (
        (
            $external_enum:ident,
            $(#[$enum_meta:meta])*
            $visibility:vis enum $internal_enum_name:ident {
                $(
                    $(#[$variant_meta:meta])*
                    $variant:ident
                ),+ $(,)?
            }
        )
    ) => {
        // Define the internal enum.
        $(#[$enum_meta])*
        $visibility enum $internal_enum_name {
            $(
                $(#[$variant_meta])*
                $variant
            ),+
        }

        // Implement conversions.
        impl From<$internal_enum_name> for $external_enum {
            fn from(value: $internal_enum_name) -> Self {
                match value {
                    $(
                        $(#[$variant_meta])*
                        $internal_enum_name::$variant => $external_enum::$variant,
                    )+
                }
            }
        }

        impl From<$external_enum> for $internal_enum_name {
            fn from(value: $external_enum) -> Self {
                match value {
                    $(
                        $(#[$variant_meta])*
                        $external_enum::$variant => $internal_enum_name::$variant,
                    )+
                }
            }
        }
    }
}

// Define the ShortKeySizeArg enum and implement conversions to and from ShortKeySize.
clone_enum_as_arg_enum!(
    (
        ShortKeySize,
        /// Key size, in bytes, for the short key storage.
        #[derive(clap::ValueEnum, Clone, PartialEq, Debug)]
        pub enum ShortKeySizeArg {
            U16,
            U17,
            U18,
            U19,
            U20,
            U21,
            U22,
            U23,
            U24,
            U25,
            U26,
            U27,
            U28,
            U29,
            U30,
            U31,
            U32,
        }
    )
);

// Define the patricia storage layout arg enum and implement conversions to and from
// PatriciaStorageLayout.
clone_enum_as_arg_enum!(
    (
        PatriciaStorageLayout,
        /// Patricia storage layout to use.
        #[derive(clap::ValueEnum, Clone, PartialEq, Debug)]
        pub enum PatriciaStorageLayoutArg {
            Fact,
        }
    )
);

#[derive(Debug, Args)]
struct StorageArgs {
    /// Seed for the random number generator.
    #[clap(short = 's', long, default_value = "42")]
    seed: u64,
    /// Number of iterations to run the benchmark.
    #[clap(long, default_value = "1000")]
    n_iterations: usize,
    /// Benchmark flavor determines the size and structure of the generated state diffs.
    #[clap(long, default_value = "1k-diff")]
    flavor: BenchmarkFlavor,
    /// Storage impl to use. Note that MapStorage isn't persisted in the file system, so
    /// checkpointing is ignored.
    #[clap(long, default_value = "cached-mdbx")]
    storage_type: StorageType,
    /// Patricia storage layout to use.
    #[clap(long, default_value = "fact")]
    storage_layout: PatriciaStorageLayoutArg,
    /// If true, the storage will use memory-mapped files. Only relevant for Rocksdb.
    /// False by default, as fact storage layout does not benefit from mapping disk pages to
    /// memory, as there is no locality of related data.
    #[clap(long, short, action=ArgAction::SetFalse)]
    allow_mmap: bool,
    /// If not none, wraps the storage in the key-shrinking storage of the given size.
    #[clap(long, default_value = None)]
    key_size: Option<ShortKeySizeArg>,
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
        $flavor:expr,
        $output_dir:expr,
        $checkpoint_dir_arg:expr,
        $storage:expr,
        $storage_layout:expr,
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
                    $storage_layout,
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
                        $storage_layout,
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
async fn run_storage_benchmark_wrapper<S: Storage>(
    StorageArgs {
        seed,
        n_iterations,
        flavor,
        storage_type,
        checkpoint_interval,
        data_path,
        output_dir,
        checkpoint_dir,
        key_size,
        storage_layout,
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
        StorageType::Mdbx
        | StorageType::CachedMdbx
        | StorageType::Rocksdb
        | StorageType::CachedRocksdb => Some(checkpoint_dir.as_str()),
        StorageType::MapStorage => None,
    };

    generate_short_key_benchmark!(
        key_size,
        seed,
        n_iterations,
        flavor,
        output_dir,
        checkpoint_dir_arg,
        storage,
        storage_layout.into(),
        checkpoint_interval,
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
