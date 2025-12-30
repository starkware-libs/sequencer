use starknet_patricia_storage::aerospike_storage::AerospikeStorageConfig;
use starknet_patricia_storage::map_storage::CachedStorageConfig;
use starknet_patricia_storage::rocksdb_storage::RocksDbOptions;
use starknet_patricia_storage::short_key_storage::ShortKeySize;

pub const DEFAULT_DATA_PATH: &str = "/mnt/data/committer_storage_benchmark";
pub const DEFAULT_STORAGE_PATH: &str = "/mnt/data/storage";

/// Defines the underlying forest storage used by the committer.
pub enum StorageLayout {
    // Fact storage layout: each key is the hash of the value.
    Fact(SingleStorageFields),
}

/// Settings for a file-backed storage.
pub struct FileBasedStorageFields {
    /// A path to a directory to store the DB, output and checkpoints unless they are
    /// explicitly provided. Defaults to "/tmp/committer_storage_benchmark/".
    pub data_path: String,

    /// A path to a directory to store the DB if needed.
    pub storage_path: String,

    /// Global fields for the storage.
    pub global_fields: SingleStorageGlobalFields,

    /// Specific database fields for the storage.
    pub specific_db_fields: SpecificDbFields,
}

pub struct SingleStorageGlobalFields {
    // If not None, the storage will be wrapped in a key-shrinking storage.
    pub short_key_size: Option<ShortKeySize>,
    // If not None, the storage will be wrapped in a cached storage.
    pub cache_fields: Option<CachedStorageConfig>,
}

/// Settings for a single storage instance. Forest layouts using more than one storage instance may
/// use separate instances of this enum.
// TODO(Dori): Remove this #[allow].
#[allow(clippy::large_enum_variant)]
pub enum SingleStorageFields {
    Memory(SingleMemoryStorageFields),
    FileBased(FileBasedStorageFields),
}

impl SingleStorageFields {
    pub fn global_fields(&self) -> &SingleStorageGlobalFields {
        match self {
            Self::Memory(SingleMemoryStorageFields(global_fields)) => global_fields,
            Self::FileBased(FileBasedStorageFields { global_fields, .. }) => global_fields,
        }
    }
}

/// Settings for a memory-backed storage.
pub struct SingleMemoryStorageFields(SingleStorageGlobalFields);

/// Settings for a specific, single, file-backed database instance.
// [AerospikeStorageConfig] is a large enum variant, so we need to allow it. Not so bad though.
#[allow(clippy::large_enum_variant)]
pub enum SpecificDbFields {
    RocksDb(RocksDbOptions),
    Mdbx(MdbxFields),
    Aerospike(AerospikeStorageConfig),
}

/// Settings for a MDBX database instance.
// TODO(Dori): Define a `MdbxStorageConfig` struct in the patricia storage crate, and use it instead
//   of this struct.
#[derive(Default)]
pub struct MdbxFields {}
