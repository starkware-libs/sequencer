use clap::{ArgAction, Args};
use starknet_patricia_storage::short_key_storage::ShortKeySize;

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
    Mdbx,
    CachedMdbx,
    Rocksdb,
    CachedRocksdb,
    Aerospike,
    CachedAerospike,
}

const DEFAULT_DATA_PATH: &str = "/tmp/committer_storage_benchmark";

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

// TODO(Dori): About time to split into subcommands by storage type... some args are only relevant
//   for certain storage types.
#[derive(Debug, Args)]
pub struct StorageArgs {
    /// Seed for the random number generator.
    #[clap(short = 's', long, default_value = "42")]
    pub seed: u64,
    /// Number of iterations to run the benchmark.
    #[clap(long, default_value = "1000")]
    pub n_iterations: usize,
    /// Benchmark flavor determines the size and structure of the generated state diffs.
    #[clap(long, default_value = "1k-diff")]
    pub flavor: BenchmarkFlavor,
    /// Storage impl to use. Note that MapStorage isn't persisted in the file system, so
    /// checkpointing is ignored.
    #[clap(long, default_value = "cached-mdbx")]
    pub storage_type: StorageType,
    /// Aerospike aeroset.
    #[clap(long, default_value = None)]
    pub aeroset: Option<String>,
    /// Aerospike namespace.
    #[clap(long, default_value = None)]
    pub namespace: Option<String>,
    /// Aerospike hosts.
    #[clap(long, default_value = None)]
    pub hosts: Option<String>,
    /// If true, the storage will use memory-mapped files. Only relevant for Rocksdb.
    /// False by default, as fact storage layout does not benefit from mapping disk pages to
    /// memory, as there is no locality of related data.
    #[clap(long, short, action=ArgAction::SetTrue)]
    pub allow_mmap: bool,
    /// If true, when using CachedStorage, statistics collection from the storage will include
    /// internal storage statistics (and not just cache stats).
    #[clap(long, action=ArgAction::SetTrue)]
    pub include_inner_stats: bool,
    /// If not none, wraps the storage in the key-shrinking storage of the given size.
    #[clap(long, default_value = None)]
    pub key_size: Option<ShortKeySizeArg>,
    /// If using cached storage, the size of the cache.
    #[clap(long, default_value = "1000000")]
    pub cache_size: usize,
    #[clap(long, default_value = "1000")]
    pub checkpoint_interval: usize,
    #[clap(long, default_value = "warn")]
    pub log_level: String,
    /// A path to a directory to store the DB, output and checkpoints unless they are
    /// explicitly provided. Defaults to "/tmp/committer_storage_benchmark/".
    #[clap(short = 'd', long, default_value = DEFAULT_DATA_PATH)]
    pub data_path: String,
    /// A path to a directory to store the DB if needed.
    #[clap(long, default_value = None)]
    pub storage_path: Option<String>,
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
