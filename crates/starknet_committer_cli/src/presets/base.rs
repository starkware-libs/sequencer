use std::num::NonZeroUsize;
use std::sync::LazyLock;

use starknet_patricia_storage::map_storage::{CachedStorageConfig, DEFAULT_CACHE_SIZE};
use tracing::Level;

use crate::presets::types::flavors::{
    BenchmarkFlavor,
    FlavorFields,
    InterferenceFields,
    InterferenceFlavor,
    DEFAULT_INTERFERENCE_CONCURRENCY_LIMIT,
};
use crate::presets::types::storage::{
    FileBasedStorageFields,
    RocksDbFields,
    SingleStorageFields,
    SingleStorageGlobalFields,
    SpecificDbFields,
    StorageLayout,
    DEFAULT_DATA_PATH,
    DEFAULT_STORAGE_PATH,
};
use crate::presets::types::PresetFields;

pub static BASE_PRESET: LazyLock<PresetFields> = LazyLock::new(|| {
    PresetFields::new(
        FlavorFields {
            data_path: DEFAULT_DATA_PATH.clone(),
            seed: 42,
            n_iterations: 1000000,
            flavor: BenchmarkFlavor::Constant,
            interference_fields: InterferenceFields {
                interference_type: InterferenceFlavor::None,
                interference_concurrency_limit: DEFAULT_INTERFERENCE_CONCURRENCY_LIMIT,
            },
            n_updates: 1000,
            checkpoint_interval: 1000,
            log_level: Level::WARN,
        },
        StorageLayout::Fact(SingleStorageFields::FileBased(FileBasedStorageFields {
            storage_path: DEFAULT_STORAGE_PATH.clone(),
            global_fields: SingleStorageGlobalFields {
                short_key_size: None,
                cache_fields: Some(CachedStorageConfig {
                    cache_size: DEFAULT_CACHE_SIZE,
                    cache_on_write: true,
                    include_inner_stats: false,
                }),
            },
            specific_db_fields: SpecificDbFields::RocksDb(RocksDbFields {
                use_column_families: false,
                allow_mmap: true,
            }),
        })),
    )
});
