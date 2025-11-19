use std::fs;

use starknet_patricia_storage::aerospike_storage::Port;
use starknet_patricia_storage::map_storage::CachedStorageConfig;
use starknet_patricia_storage::short_key_storage::ShortKeySize;

pub const DEFAULT_DATA_PATH: &str = "/mnt/data/committer_storage_benchmark";
pub const DEFAULT_STORAGE_PATH: &str = "/mnt/data/storage";

/// Defines the underlying forest storage used by the committer.
#[derive(Debug)]
pub enum StorageLayout {
    // Fact storage layout: each key is the hash of the value.
    Fact(SingleStorageFields),
}

pub trait StorageLayoutName {
    fn short_name(&self) -> String;
}

impl StorageLayout {
    pub fn supports_interference(&self) -> bool {
        // All file-backed storages support interference, unless it's wrapped in a cached storage.
        let Self::Fact(SingleStorageFields::FileBased(FileBasedStorageFields {
            ref global_fields,
            ..
        })) = self
        else {
            return false;
        };
        global_fields.cache_fields.is_none()
    }
}

impl StorageLayoutName for StorageLayout {
    fn short_name(&self) -> String {
        match self {
            Self::Fact(fact_storage) => fact_storage.short_name(),
        }
    }
}

/// Settings for a file-backed storage.
#[derive(Debug)]
pub struct FileBasedStorageFields {
    /// A path to a directory to store the DB if needed.
    pub storage_path: String,

    /// Global fields for the storage.
    pub global_fields: SingleStorageGlobalFields,

    /// Specific database fields for the storage.
    pub specific_db_fields: SpecificDbFields,
}

impl FileBasedStorageFields {
    pub fn initialize_storage_path(&self) {
        fs::create_dir_all(&self.storage_path).expect("Failed to create storage directory.");
    }
}

#[derive(Debug)]
pub struct SingleStorageGlobalFields {
    // If not None, the storage will be wrapped in a key-shrinking storage.
    pub short_key_size: Option<ShortKeySize>,
    // If not None, the storage will be wrapped in a cached storage.
    pub cache_fields: Option<CachedStorageConfig>,
}

impl StorageLayoutName for SingleStorageGlobalFields {
    fn short_name(&self) -> String {
        let short_key_name = self.short_key_size.as_ref().map(|_| "shortkey").unwrap_or("");
        let cache_name = self.cache_fields.as_ref().map(|_| "cached").unwrap_or("");
        format!("{short_key_name}_{cache_name}")
    }
}

/// Settings for a single storage instance. Forest layouts using more than one storage instance may
/// use separate instances of this enum.
#[derive(Debug)]
pub enum SingleStorageFields {
    Memory(SingleMemoryStorageFields),
    FileBased(FileBasedStorageFields),
}

impl StorageLayoutName for SingleStorageFields {
    fn short_name(&self) -> String {
        match self {
            Self::Memory(SingleMemoryStorageFields(global_fields)) => {
                format!("memory_{}", global_fields.short_name())
            }
            Self::FileBased(FileBasedStorageFields {
                global_fields, specific_db_fields, ..
            }) => {
                format!("{}_{}", specific_db_fields.short_name(), global_fields.short_name())
            }
        }
    }
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
#[derive(Debug)]
pub struct SingleMemoryStorageFields(pub SingleStorageGlobalFields);

/// Settings for a specific, single, file-backed database instance.
#[derive(Debug)]
pub enum SpecificDbFields {
    RocksDb(RocksDbFields),
    Mdbx(MdbxFields),
    Aerospike(AerospikeFields),
}

impl StorageLayoutName for SpecificDbFields {
    fn short_name(&self) -> String {
        match self {
            Self::RocksDb(_) => "rocksdb",
            Self::Mdbx(_) => "mdbx",
            Self::Aerospike(_) => "aerospike",
        }
        .to_string()
    }
}

/// Settings for a MDBX database instance.
// TODO(Dori): Define a `MdbxStorageConfig` struct in the patricia storage crate, and use it instead
//   of this struct.
#[derive(Default, Debug)]
pub struct MdbxFields {}

/// Configuration settings for a RocksDB database instance.
#[derive(Debug)]
pub struct RocksDbFields {
    pub use_column_families: bool,
    pub allow_mmap: bool,
}

/// Configuration settings for a Aerospike database instance.
#[derive(Debug)]
pub struct AerospikeFields {
    pub aeroset: String,
    pub namespace: String,
    pub hosts: Vec<(String, Port)>,
}
