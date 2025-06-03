use std::path::{Path, PathBuf};

use blockifier::context::ChainInfo;
use mempool_test_utils::starknet_api_test_utils::AccountTransactionGenerator;

use crate::executable_setup::NodeExecutionId;
use crate::state_reader::{
    StorageTestConfig,
    StorageTestSetup,
    BATCHER_DB_PATH_SUFFIX,
    CLASSES_STORAGE_DB_PATH_SUFFIX,
    CLASS_HASH_STORAGE_DB_PATH_SUFFIX,
    CLASS_MANAGER_DB_PATH_SUFFIX,
    STATE_SYNC_DB_PATH_SUFFIX,
};

#[derive(Debug)]
pub struct StorageExecutablePaths {
    batcher_path: PathBuf,
    state_sync_path: PathBuf,
    class_manager_path: PathBuf,
}

impl StorageExecutablePaths {
    pub fn new(
        db_base: &Path,
        node_index: usize,
        batcher_index: usize,
        state_sync_index: usize,
        class_manager_index: usize,
    ) -> Self {
        let batcher_node_index = NodeExecutionId::new(node_index, batcher_index);
        let state_sync_node_index = NodeExecutionId::new(node_index, state_sync_index);
        let class_manager_node_index = NodeExecutionId::new(node_index, class_manager_index);

        let batcher_path = batcher_node_index.build_path(db_base);
        let state_sync_path = state_sync_node_index.build_path(db_base);
        let class_manager_path = class_manager_node_index.build_path(db_base);

        Self { batcher_path, state_sync_path, class_manager_path }
    }

    pub fn get_batcher_exec_path(&self) -> &PathBuf {
        &self.batcher_path
    }

    pub fn get_state_sync_exec_path(&self) -> &PathBuf {
        &self.state_sync_path
    }

    pub fn get_class_manager_exec_path(&self) -> &PathBuf {
        &self.class_manager_path
    }

    pub fn get_batcher_path_with_db_suffix(&self) -> PathBuf {
        self.batcher_path.join(BATCHER_DB_PATH_SUFFIX)
    }

    pub fn get_state_sync_path_with_db_suffix(&self) -> PathBuf {
        self.state_sync_path.join(STATE_SYNC_DB_PATH_SUFFIX)
    }

    pub fn get_class_manager_path_with_db_suffix(&self) -> PathBuf {
        self.class_manager_path.join(CLASS_MANAGER_DB_PATH_SUFFIX)
    }
}

#[derive(Debug, Clone)]
pub struct CustomPaths {
    db_base: Option<PathBuf>,
    config_base: Option<PathBuf>,
    data_prefix_base: Option<PathBuf>,
}

impl CustomPaths {
    pub fn new(
        db_base: Option<PathBuf>,
        config_base: Option<PathBuf>,
        data_prefix_base: Option<PathBuf>,
    ) -> Self {
        Self { db_base, config_base, data_prefix_base }
    }

    pub fn get_db_base(&self) -> Option<&PathBuf> {
        self.db_base.as_ref()
    }

    pub fn get_config_path(&self, node_execution_id: &NodeExecutionId) -> Option<PathBuf> {
        self.config_base.as_ref().map(|p| node_execution_id.build_path(p))
    }

    pub fn get_data_prefix_path(&self) -> Option<&PathBuf> {
        self.data_prefix_base.as_ref()
    }
}

pub fn get_integration_test_storage(
    node_index: usize,
    batcher_index: usize,
    state_sync_index: usize,
    class_manager_index: usize,
    custom_paths: Option<CustomPaths>,
    accounts: Vec<AccountTransactionGenerator>,
    chain_info: &ChainInfo,
) -> StorageTestSetup {
    let storage_exec_paths = custom_paths.as_ref().and_then(|paths| {
        paths.get_db_base().map(|db_base| {
            StorageExecutablePaths::new(
                db_base,
                node_index,
                batcher_index,
                state_sync_index,
                class_manager_index,
            )
        })
    });

    let StorageTestSetup { mut storage_config, storage_handles } =
        StorageTestSetup::new(accounts, chain_info, storage_exec_paths);

    // Allow overriding the path with a custom prefix for Docker mode in system tests.
    if let Some(paths) = custom_paths {
        if let Some(prefix) = paths.get_data_prefix_path() {
            let custom_storage_exec_paths = StorageExecutablePaths::new(
                prefix,
                node_index,
                batcher_index,
                state_sync_index,
                class_manager_index,
            );
            storage_config.batcher_storage_config.db_config.path_prefix =
                custom_storage_exec_paths.get_batcher_exec_path().join(BATCHER_DB_PATH_SUFFIX);
            storage_config.state_sync_storage_config.db_config.path_prefix =
                custom_storage_exec_paths
                    .get_state_sync_exec_path()
                    .join(STATE_SYNC_DB_PATH_SUFFIX);
            let s_s = storage_config.state_sync_storage_config.db_config.path_prefix.clone();
            print!("storage_config.state_sync_storage_config.db_config.path_prefix is {:?}", s_s);
            storage_config
                .class_manager_storage_config
                .class_hash_storage_config
                .class_hash_db_config
                .path_prefix = custom_storage_exec_paths
                .get_class_manager_exec_path()
                .join(CLASS_MANAGER_DB_PATH_SUFFIX)
                .join(CLASS_HASH_STORAGE_DB_PATH_SUFFIX);
            storage_config.class_manager_storage_config.persistent_root = custom_storage_exec_paths
                .get_class_manager_exec_path()
                .join(CLASS_MANAGER_DB_PATH_SUFFIX)
                .join(CLASSES_STORAGE_DB_PATH_SUFFIX);
        }
    }

    StorageTestSetup {
        storage_config: StorageTestConfig::new(
            storage_config.batcher_storage_config,
            storage_config.state_sync_storage_config,
            storage_config.class_manager_storage_config,
        ),
        storage_handles,
    }
}
