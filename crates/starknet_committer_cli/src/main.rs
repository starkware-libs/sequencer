use std::fs;
use std::num::NonZeroUsize;
use std::path::Path;
use rand::Rng;
use sha3::{Digest, Keccak256};


use apollo_infra_utils::tracing_utils::{configure_tracing, modify_log_level};
use clap::{Args, Parser, Subcommand};
use starknet_committer_cli::commands::run_storage_benchmark;
use starknet_patricia_storage::map_storage::{CachedStorage, MapStorage};
use starknet_patricia_storage::mdbx_storage::MdbxStorage;
use starknet_patricia_storage::rocksdb_storage::RocksdbStorage;
use starknet_patricia_storage::storage_trait::{DbKey, DbValue, Storage};
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

pub async fn run_committer_cli(
    committer_command: CommitterCliCommand,
    log_filter_handle: Handle<LevelFilter, Registry>,
) {
    info!("Starting committer-cli with command: \n{:?}", committer_command);
    match committer_command.command {
        Command::StorageBenchmark(StorageArgs {
            seed,
            n_iterations,
            n_diffs,
            storage_type,
            cache_size,
            checkpoint_interval,
            log_level,
            data_path,
            storage_path,
            output_dir,
            checkpoint_dir,
        }) => {
            modify_log_level(log_level, log_filter_handle);
            let output_dir = output_dir
                .unwrap_or_else(|| format!("{data_path}/{storage_type:?}/csvs/{n_iterations}"));
            let checkpoint_dir = checkpoint_dir.unwrap_or_else(|| {
                format!("{data_path}/{storage_type:?}/checkpoints/{n_iterations}")
            });

            match storage_type {
                StorageType::MapStorage => {
                    let storage = MapStorage::default();
                    run_storage_benchmark(
                        seed,
                        n_iterations,
                        n_diffs,
                        &output_dir,
                        None,
                        storage,
                        checkpoint_interval,
                    )
                    .await;
                }
                StorageType::Mdbx => {
                    let storage_path = storage_path
                        .unwrap_or_else(|| format!("{data_path}/storage/{storage_type:?}"));
                    fs::create_dir_all(&storage_path).expect("Failed to create storage directory.");
                    let storage = MdbxStorage::open(Path::new(&storage_path)).unwrap();
                    run_storage_benchmark(
                        seed,
                        n_iterations,
                        n_diffs,
                        &output_dir,
                        Some(&checkpoint_dir),
                        storage,
                        checkpoint_interval,
                    )
                    .await;
                }
                StorageType::CachedMdbx => {
                    let storage_path = storage_path
                        .unwrap_or_else(|| format!("{data_path}/storage/{storage_type:?}"));
                    fs::create_dir_all(&storage_path).expect("Failed to create storage directory.");
                    let storage = CachedStorage::new(
                        MdbxStorage::open(Path::new(&storage_path)).unwrap(),
                        NonZeroUsize::new(cache_size).unwrap(),
                    );
                    run_storage_benchmark(
                        seed,
                        n_iterations,
                        n_diffs,
                        &output_dir,
                        Some(&checkpoint_dir),
                        storage,
                        checkpoint_interval,
                    )
                    .await;
                }
                StorageType::Rocksdb => {
                    let storage_path = storage_path
                        .unwrap_or_else(|| format!("{data_path}/storage/{storage_type:?}"));
                    fs::create_dir_all(&storage_path).expect("Failed to create storage directory.");
                    let mut storage = RocksdbStorage::open(Path::new(&storage_path)).unwrap();
                    memory_db_test(&mut storage).await;
                    // run_storage_benchmark(
                    //     seed,
                    //     n_iterations,
                    //     n_diffs,
                    //     &output_dir,
                    //     Some(&checkpoint_dir),
                    //     storage,
                    //     checkpoint_interval,
                    // )
                    // .await;
                }
            }
        }
    }
}

pub const NUM_READS: u32 = 10000;

pub async fn memory_db_test(db: &mut RocksdbStorage) {

    let mut i: u32 = 1;
    while i > 0 {
        info!("iteration: {}", i);

        let mut rng = rand::thread_rng();

        // read NUM_READS random keys of the form h(x) where x is in the range [0, i+1k]
        let random_preimages_to_read = (0..NUM_READS).map(|_| rng.gen_range(0..(i+1000)));
        let keys = random_preimages_to_read.map(|r| {
            let mut hash_state = Keccak256::default();
            hash_state.update(r.to_be_bytes().to_vec());
            DbKey(hash_state.finalize().to_vec())
        }).collect::<Vec<DbKey>>();
        let key_refs: Vec<&DbKey> = keys.iter().collect();
        let values = db.mget(key_refs.as_slice()).unwrap();

        // write h(i)
        let mut hash_state = Keccak256::default();
        hash_state.update(i.to_be_bytes().to_vec());
        let key = DbKey(hash_state.finalize().to_vec());
        let res = db.set(key, DbValue(i.to_be_bytes().to_vec())).unwrap();

        i += 1;
    }
}

#[tokio::main]
async fn main() {
    let log_filter_handle = configure_tracing();
    let committer_command = CommitterCliCommand::parse();
    run_committer_cli(committer_command, log_filter_handle).await;
}
