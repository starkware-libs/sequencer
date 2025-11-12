use std::fs;
use std::num::NonZeroUsize;
use std::path::Path;

use clap::{ArgAction, Args, Subcommand};
use starknet_patricia_storage::aerospike_storage::{AerospikeStorage, AerospikeStorageConfig};
use starknet_patricia_storage::map_storage::{CachedStorage, CachedStorageConfig, MapStorage};
use starknet_patricia_storage::mdbx_storage::MdbxStorage;
use starknet_patricia_storage::rocksdb_storage::{RocksDbOptions, RocksDbStorage};
use starknet_patricia_storage::short_key_storage::ShortKeySize;
use starknet_patricia_storage::storage_trait::Storage;

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

#[derive(clap::ValueEnum, Clone, PartialEq, Debug)]
pub enum StorageType {
    MapStorage,
    CachedMapStorage,
    Mdbx,
    CachedMdbx,
    Rocksdb,
    CachedRocksdb,
    Aerospike,
    CachedAerospike,
}

pub const DEFAULT_DATA_PATH: &str = "/tmp/committer_storage_benchmark";

pub trait StorageFromArgs: Args {
    fn storage(&self) -> impl Storage;
}

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

/// Define this conversion to make sure the arg-enum matches the original enum.
/// The original enum defines the possible sizes, but we do not want to implement ValueEnum for it.
impl From<ShortKeySizeArg> for ShortKeySize {
    fn from(arg: ShortKeySizeArg) -> Self {
        match arg {
            ShortKeySizeArg::U16 => Self::U16,
            ShortKeySizeArg::U17 => Self::U17,
            ShortKeySizeArg::U18 => Self::U18,
            ShortKeySizeArg::U19 => Self::U19,
            ShortKeySizeArg::U20 => Self::U20,
            ShortKeySizeArg::U21 => Self::U21,
            ShortKeySizeArg::U22 => Self::U22,
            ShortKeySizeArg::U23 => Self::U23,
            ShortKeySizeArg::U24 => Self::U24,
            ShortKeySizeArg::U25 => Self::U25,
            ShortKeySizeArg::U26 => Self::U26,
            ShortKeySizeArg::U27 => Self::U27,
            ShortKeySizeArg::U28 => Self::U28,
            ShortKeySizeArg::U29 => Self::U29,
            ShortKeySizeArg::U30 => Self::U30,
            ShortKeySizeArg::U31 => Self::U31,
            ShortKeySizeArg::U32 => Self::U32,
        }
    }
}

#[derive(Debug, Args)]
pub struct GlobalArgs {
    /// Seed for the random number generator.
    #[clap(short = 's', long, default_value = "42")]
    pub seed: u64,

    /// Number of iterations to run the benchmark.
    #[clap(long, default_value = "1000")]
    pub n_iterations: usize,

    /// Benchmark flavor determines the size and structure of the generated state diffs.
    #[clap(long, default_value = "1k-diff")]
    pub flavor: BenchmarkFlavor,

    /// If not none, wraps the storage in the key-shrinking storage of the given size.
    #[clap(long, default_value = None)]
    pub key_size: Option<ShortKeySizeArg>,

    /// Interval at which to save checkpoints.
    #[clap(long, default_value = "1000")]
    pub checkpoint_interval: usize,

    /// Log level.
    #[clap(long, default_value = "warn")]
    pub log_level: String,

    /// A path to a directory to store the csv outputs. If not given, creates a dir according to
    /// the  n_iterations (i.e., rwo runs with different n_iterations will have different csv
    /// outputs)
    #[clap(long, default_value = None)]
    pub output_dir: Option<String>,

    /// A path to a directory to store the checkpoints to allow benchmark recovery. If not given,
    /// creates a dir according to the n_iterations (i.e., two runs with different n_iterations
    /// will have different checkpoints)
    #[clap(long, default_value = None)]
    pub checkpoint_dir: Option<String>,
}

#[derive(Debug, Args)]
pub struct FileStorageArgs {
    /// A path to a directory to store the DB, output and checkpoints unless they are
    /// explicitly provided. Defaults to "/tmp/committer_storage_benchmark/".
    #[clap(short = 'd', long, default_value = DEFAULT_DATA_PATH)]
    pub data_path: String,

    /// A path to a directory to store the DB if needed.
    #[clap(long, default_value = None)]
    pub storage_path: Option<String>,
}

impl FileStorageArgs {
    pub fn initialize_storage_path(&self, storage_type: StorageType) -> String {
        let path = self
            .storage_path
            .as_ref()
            .cloned()
            .unwrap_or_else(|| format!("{}/storage/{storage_type:?}", self.data_path));
        fs::create_dir_all(&path).expect("Failed to create storage directory.");
        path
    }
}

#[derive(Debug, Args)]
pub struct CachedStorageArgs<A: StorageFromArgs> {
    #[clap(flatten)]
    pub storage_args: A,

    /// If true, statistics collection from the storage will include internal storage statistics
    /// (and not just cache stats).
    #[clap(long, action=ArgAction::SetTrue)]
    pub include_inner_stats: bool,

    /// The size of the cache.
    #[clap(long, default_value = "1000000")]
    pub cache_size: usize,
}

impl<A: StorageFromArgs> StorageFromArgs for CachedStorageArgs<A> {
    fn storage(&self) -> impl Storage {
        CachedStorage::new(self.storage_args.storage(), self.cached_storage_config())
    }
}

impl<A: StorageFromArgs> CachedStorageArgs<A> {
    pub fn cache_size(&self) -> NonZeroUsize {
        NonZeroUsize::new(self.cache_size).unwrap()
    }

    pub fn cached_storage_config(&self) -> CachedStorageConfig {
        CachedStorageConfig {
            cache_size: self.cache_size(),
            cache_on_write: true,
            include_inner_stats: self.include_inner_stats,
        }
    }
}

#[derive(Debug, Args)]
pub struct MemoryArgs {
    #[clap(flatten)]
    pub global_args: GlobalArgs,
}

impl StorageFromArgs for MemoryArgs {
    fn storage(&self) -> impl Storage {
        MapStorage::default()
    }
}

#[derive(Debug, Args)]
pub struct MdbxArgs {
    #[clap(flatten)]
    pub global_args: GlobalArgs,
    #[clap(flatten)]
    pub file_storage_args: FileStorageArgs,
}

impl StorageFromArgs for MdbxArgs {
    fn storage(&self) -> impl Storage {
        MdbxStorage::open(Path::new(
            &self.file_storage_args.initialize_storage_path(StorageType::Mdbx),
        ))
        .unwrap()
    }
}

#[derive(Debug, Args)]
pub struct RocksdbArgs {
    #[clap(flatten)]
    pub global_args: GlobalArgs,
    #[clap(flatten)]
    pub file_storage_args: FileStorageArgs,

    /// If true, the storage will use memory-mapped files.
    /// False by default, as fact storage layout does not benefit from mapping disk pages to
    /// memory, as there is no locality of related data.
    #[clap(long, short, action=ArgAction::SetTrue)]
    pub allow_mmap: bool,
}

impl StorageFromArgs for RocksdbArgs {
    fn storage(&self) -> impl Storage {
        RocksDbStorage::open(
            Path::new(&self.file_storage_args.initialize_storage_path(StorageType::Rocksdb)),
            self.rocksdb_options(),
        )
        .unwrap()
    }
}

impl RocksdbArgs {
    pub fn rocksdb_options(&self) -> RocksDbOptions {
        if self.allow_mmap { RocksDbOptions::default() } else { RocksDbOptions::default_no_mmap() }
    }
}

#[derive(Debug, Args)]
pub struct AerospikeArgs {
    #[clap(flatten)]
    pub global_args: GlobalArgs,
    #[clap(flatten)]
    pub file_storage_args: FileStorageArgs,

    /// Aerospike aeroset.
    #[clap(long)]
    pub aeroset: String,

    /// Aerospike namespace.
    #[clap(long)]
    pub namespace: String,

    /// Aerospike hosts.
    #[clap(long)]
    pub hosts: String,
}

impl StorageFromArgs for AerospikeArgs {
    fn storage(&self) -> impl Storage {
        AerospikeStorage::new(self.aerospike_storage_config()).unwrap()
    }
}

impl AerospikeArgs {
    pub fn aerospike_storage_config(&self) -> AerospikeStorageConfig {
        AerospikeStorageConfig::new_default(
            self.aeroset.clone(),
            self.namespace.clone(),
            self.hosts.clone(),
        )
    }
}

#[derive(Debug, Subcommand)]
pub enum StorageBenchmarkCommand {
    Memory(MemoryArgs),
    CachedMemory(CachedStorageArgs<MemoryArgs>),
    Mdbx(MdbxArgs),
    CachedMdbx(CachedStorageArgs<MdbxArgs>),
    Rocksdb(RocksdbArgs),
    CachedRocksdb(CachedStorageArgs<RocksdbArgs>),
    Aerospike(AerospikeArgs),
    CachedAerospike(CachedStorageArgs<AerospikeArgs>),
}

impl StorageBenchmarkCommand {
    pub fn global_args(&self) -> &GlobalArgs {
        match self {
            Self::Memory(args) => &args.global_args,
            Self::CachedMemory(args) => &args.storage_args.global_args,
            Self::Mdbx(args) => &args.global_args,
            Self::CachedMdbx(args) => &args.storage_args.global_args,
            Self::Rocksdb(args) => &args.global_args,
            Self::CachedRocksdb(args) => &args.storage_args.global_args,
            Self::Aerospike(args) => &args.global_args,
            Self::CachedAerospike(args) => &args.storage_args.global_args,
        }
    }

    pub fn file_storage_args(&self) -> Option<&FileStorageArgs> {
        match self {
            Self::Memory(_) | Self::CachedMemory(_) => None,
            Self::Mdbx(args) => Some(&args.file_storage_args),
            Self::CachedMdbx(args) => Some(&args.storage_args.file_storage_args),
            Self::Rocksdb(args) => Some(&args.file_storage_args),
            Self::CachedRocksdb(args) => Some(&args.storage_args.file_storage_args),
            Self::Aerospike(args) => Some(&args.file_storage_args),
            Self::CachedAerospike(args) => Some(&args.storage_args.file_storage_args),
        }
    }

    pub fn storage_type(&self) -> StorageType {
        match self {
            Self::Memory(_) => StorageType::MapStorage,
            Self::CachedMemory(_) => StorageType::MapStorage,
            Self::Mdbx(_) => StorageType::Mdbx,
            Self::CachedMdbx(_) => StorageType::CachedMdbx,
            Self::Rocksdb(_) => StorageType::Rocksdb,
            Self::CachedRocksdb(_) => StorageType::CachedRocksdb,
            Self::Aerospike(_) => StorageType::Aerospike,
            Self::CachedAerospike(_) => StorageType::CachedAerospike,
        }
    }
}
