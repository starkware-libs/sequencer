use std::num::NonZeroUsize;
use std::sync::LazyLock;

use starknet_patricia_storage::map_storage::CachedStorageConfig;
use starknet_patricia_storage::rocksdb_storage::RocksDbOptions;
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
    SingleStorageFields,
    SingleStorageGlobalFields,
    SpecificDbFields,
    StorageLayout,
};
use crate::presets::types::PresetFields;

pub static BASE_PRESET: LazyLock<PresetFields> = LazyLock::new(|| {
    PresetFields::new(
        FlavorFields {
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
            data_path: "/mnt/data/committer_storage_benchmark".to_string(),
            storage_path: "/mnt/data/storage".to_string(),
            global_fields: SingleStorageGlobalFields {
                short_key_size: None,
                cache_fields: Some(CachedStorageConfig {
                    cache_size: NonZeroUsize::new(10_000_000).unwrap(),
                    cache_on_write: true,
                    include_inner_stats: false,
                }),
            },
            specific_db_fields: SpecificDbFields::RocksDb(RocksDbOptions::default()),
        })),
    )
});
