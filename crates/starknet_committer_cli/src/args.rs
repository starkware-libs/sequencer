use std::fs;
use std::num::NonZeroUsize;
use std::path::Path;

use starknet_patricia_storage::aerospike_storage::{AerospikeStorage, AerospikeStorageConfig};
use starknet_patricia_storage::map_storage::{CachedStorage, CachedStorageConfig, MapStorage};
use starknet_patricia_storage::mdbx_storage::MdbxStorage;
use starknet_patricia_storage::rocksdb_storage::{RocksDbOptions, RocksDbStorage};
use starknet_patricia_storage::short_key_storage::ShortKeySize;
use starknet_patricia_storage::storage_trait::Storage;

use crate::commands::run_storage_benchmark_wrapper;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BenchmarkFlavor {
    /// Constant number of updates per iteration.
    Constant,
    /// Periodic peaks of a constant number of updates per peak iteration, with 20% of the number
    /// of updates on non-peak iterations. Peaks are 10 iterations every 500 iterations.
    PeriodicPeaks,
    /// Constant number of state diffs per iteration, with 20% new leaves per iteration. The other
    /// 80% leaf updates are sampled randomly from recent leaf updates.
    /// For the first blocks, behaves just like [Self::Constant] ("warmup" phase).
    Overlap,
    /// Constant number of updates per iteration, where block N generates updates for leaf keys
    /// [N * C, (N + 1) * C).
    Continuous,
}

#[derive(Clone, PartialEq, Debug)]
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

pub const DEFAULT_DATA_PATH: &str = "/mnt/data/committer_storage_benchmark";

pub trait StorageFromArgs: Default {
    fn storage(&self) -> impl Storage;
}

#[derive(Debug)]
pub struct GlobalArgs {
    /// Seed for the random number generator.
    pub seed: u64,

    /// Number of iterations to run the benchmark.
    pub n_iterations: usize,

    /// Benchmark flavor determines the size and structure of the generated state diffs.
    pub flavor: BenchmarkFlavor,

    /// Number of updates per iteration, where applicable. Different flavors treat this value
    /// differently, see [BenchmarkFlavor] for more details.
    pub n_updates: usize,

    /// If not none, wraps the storage in the key-shrinking storage of the given size.
    pub key_size: Option<ShortKeySize>,

    /// Interval at which to save checkpoints.
    pub checkpoint_interval: usize,

    /// Log level.
    pub log_level: String,

    /// A path to a directory to store the csv outputs. If not given, creates a dir according to
    /// the  n_iterations (i.e., rwo runs with different n_iterations will have different csv
    /// outputs)
    pub output_dir: Option<String>,

    /// A path to a directory to store the checkpoints to allow benchmark recovery. If not given,
    /// creates a dir according to the n_iterations (i.e., two runs with different n_iterations
    /// will have different checkpoints)
    pub checkpoint_dir: Option<String>,
}

impl Default for GlobalArgs {
    fn default() -> Self {
        // TODO(Dori): Name these constants.
        Self {
            seed: 42,
            n_iterations: 1000000,
            flavor: BenchmarkFlavor::Constant,
            n_updates: 1000,
            key_size: None,
            checkpoint_interval: 1000,
            // TODO(Dori): Use a non-string log level type.
            log_level: "info".to_string(),
            output_dir: Some("/mnt/data/csvs".to_string()),
            checkpoint_dir: Some("/mnt/data/checkpoints".to_string()),
        }
    }
}

#[derive(Debug)]
pub struct FileStorageArgs {
    /// A path to a directory to store the DB, output and checkpoints unless they are
    /// explicitly provided. Defaults to "/tmp/committer_storage_benchmark/".
    pub data_path: String,

    /// A path to a directory to store the DB if needed.
    pub storage_path: Option<String>,
}

impl Default for FileStorageArgs {
    fn default() -> Self {
        Self {
            data_path: DEFAULT_DATA_PATH.to_string(),
            storage_path: Some("/mnt/data/storage".to_string()),
        }
    }
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

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum InterferenceType {
    /// No interference.
    None,
    /// Read 1000 random keys every block.
    Read1KEveryBlock,
}

#[derive(Debug, Clone)]
pub struct InterferenceArgs {
    /// The type of interference to apply.
    pub interference_type: InterferenceType,

    /// The maximum number of interference tasks to run concurrently.
    /// Any attempt to spawn a new interference task will log a warning and not spawn the task.
    pub interference_concurrency_limit: usize,
}

impl Default for InterferenceArgs {
    fn default() -> Self {
        Self { interference_type: InterferenceType::None, interference_concurrency_limit: 20 }
    }
}

#[derive(Debug)]
pub struct CachedStorageArgs<A: StorageFromArgs> {
    pub storage_args: A,

    /// If true, statistics collection from the storage will include internal storage statistics
    /// (and not just cache stats).
    pub include_inner_stats: bool,

    /// The size of the cache.
    pub cache_size: usize,
}

impl<A: StorageFromArgs> Default for CachedStorageArgs<A> {
    fn default() -> Self {
        Self { storage_args: A::default(), include_inner_stats: false, cache_size: 10000000 }
    }
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

#[derive(Debug, Default)]
pub struct MemoryArgs {
    pub global_args: GlobalArgs,
}

impl StorageFromArgs for MemoryArgs {
    fn storage(&self) -> impl Storage {
        MapStorage::default()
    }
}

#[derive(Debug, Default)]
pub struct MdbxArgs {
    pub global_args: GlobalArgs,
    pub file_storage_args: FileStorageArgs,
    pub interference_args: InterferenceArgs,
}

impl StorageFromArgs for MdbxArgs {
    fn storage(&self) -> impl Storage {
        MdbxStorage::open(Path::new(
            &self.file_storage_args.initialize_storage_path(StorageType::Mdbx),
        ))
        .unwrap()
    }
}

#[derive(Debug)]
pub struct RocksdbArgs {
    pub global_args: GlobalArgs,
    pub file_storage_args: FileStorageArgs,
    pub interference_args: InterferenceArgs,

    /// If true, the storage will use memory-mapped files.
    /// False by default, as fact storage layout does not benefit from mapping disk pages to
    /// memory, as there is no locality of related data.
    pub allow_mmap: bool,

    /// If true, the storage will use column families.
    /// False by default.
    pub use_column_families: bool,
}

impl Default for RocksdbArgs {
    fn default() -> Self {
        Self {
            global_args: GlobalArgs::default(),
            file_storage_args: FileStorageArgs::default(),
            interference_args: InterferenceArgs::default(),
            allow_mmap: true,
            use_column_families: false,
        }
    }
}

impl StorageFromArgs for RocksdbArgs {
    fn storage(&self) -> impl Storage {
        RocksDbStorage::open(
            Path::new(&self.file_storage_args.initialize_storage_path(StorageType::Rocksdb)),
            self.rocksdb_options(),
            self.use_column_families,
        )
        .unwrap()
    }
}

impl RocksdbArgs {
    pub fn rocksdb_options(&self) -> RocksDbOptions {
        if self.allow_mmap { RocksDbOptions::default() } else { RocksDbOptions::default_no_mmap() }
    }
}

#[derive(Debug)]
pub struct AerospikeArgs {
    pub global_args: GlobalArgs,
    pub file_storage_args: FileStorageArgs,
    pub interference_args: InterferenceArgs,

    /// Aerospike aeroset.
    pub aeroset: String,

    /// Aerospike namespace.
    pub namespace: String,

    /// Aerospike hosts.
    pub hosts: String,
}

impl Default for AerospikeArgs {
    fn default() -> Self {
        // TODO(Dori): Name these constants and use better default values.
        Self {
            global_args: GlobalArgs::default(),
            file_storage_args: FileStorageArgs::default(),
            interference_args: InterferenceArgs::default(),
            aeroset: "test".to_string(),
            namespace: "test".to_string(),
            hosts: "test".to_string(),
        }
    }
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

/// Utility macro to define a storage benchmark command enum and implement the
/// [StorageBenchmarkCommand::run_benchmark] method. The method itself uses a `match` with identical
/// arm implementations for each variant; explicit arm implementations are required to avoid dynamic
/// dispatch of the [Storage] type.
macro_rules! define_storage_benchmark_command {
    (
        $(#[$enum_meta:meta])*
        $visibility:vis enum $enum_name:ident {
            $( $variant:ident($args_type:ty) ),+ $(,)?
        }
    ) => {
        $(#[$enum_meta])*
        $visibility enum $enum_name {
            $( $variant($args_type), )+
        }

        #[derive(clap::ValueEnum, Debug, Clone, Copy)]
        $visibility enum Preset {
            $( $variant, )+
        }

        pub fn default_preset(preset: Preset) -> $enum_name {
            match preset {
                $( Preset::$variant => $enum_name::$variant(Default::default()), )+
            }
        }

        impl $enum_name {
            /// Run the storage benchmark.
            pub async fn run_benchmark(&self) {
                // Explicitly create a different concrete storage type in each match arm to avoid
                // dynamic dispatch.
                match self {
                    $(
                        Self::$variant(args) => {
                            let storage = args.storage();
                            run_storage_benchmark_wrapper(self, storage).await;
                        }
                    )+
                }
            }
        }
    };
}

define_storage_benchmark_command! {
    #[derive(Debug)]
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

    pub fn interference_args(&self) -> InterferenceArgs {
        match self {
            Self::Memory(_)
            | Self::CachedMemory(_)
            | Self::CachedMdbx(_)
            | Self::CachedRocksdb(_)
            | Self::CachedAerospike(_) => InterferenceArgs {
                interference_type: InterferenceType::None,
                interference_concurrency_limit: 0,
            },
            Self::Mdbx(args) => args.interference_args.clone(),
            Self::Rocksdb(args) => args.interference_args.clone(),
            Self::Aerospike(args) => args.interference_args.clone(),
        }
    }
}
