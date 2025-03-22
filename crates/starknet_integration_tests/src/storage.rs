use std::path::{Path, PathBuf};

use crate::executable_setup::NodeExecutionId;
use crate::state_reader::{
    BATCHER_DB_PATH_SUFFIX,
    CLASS_MANAGER_DB_PATH_SUFFIX,
    STATE_SYNC_DB_PATH_SUFFIX,
};
// TODO(Nadin): remove Clone derive.
#[derive(Debug, Clone)]
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

    pub fn get_data_prefix_path(&self, node_execution_id: &NodeExecutionId) -> Option<PathBuf> {
        self.data_prefix_base.as_ref().map(|p| node_execution_id.build_path(p))
    }
}
