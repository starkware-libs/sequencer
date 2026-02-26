use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::Arc;

use apollo_class_manager_config::config::FsClassStorageConfig;
use apollo_infra_utils::test_utils::{AvailablePorts, TestIdentifier};
use apollo_storage::db::DbConfig;
use apollo_storage::storage_reader_server::{
    DynamicConfigError,
    DynamicConfigProvider,
    ServerConfig,
    SharedDynamicConfigProvider,
    StorageReaderServerDynamicConfig,
};
use async_trait::async_trait;
use starknet_api::core::ChainId;
use tempfile::TempDir;

use crate::class_storage::{ClassHashStorage, FsClassStorage};

struct TestDynamicConfigProvider {
    enabled: bool,
}

#[async_trait]
impl DynamicConfigProvider for TestDynamicConfigProvider {
    async fn get_storage_reader_dynamic_config(
        &self,
    ) -> Result<StorageReaderServerDynamicConfig, DynamicConfigError> {
        Ok(StorageReaderServerDynamicConfig { enable: self.enabled })
    }
}

fn test_provider(enabled: bool) -> SharedDynamicConfigProvider {
    Arc::new(TestDynamicConfigProvider { enabled })
}

pub type FileHandles = (TempDir, TempDir);

fn available_ports_factory(instance_index: u16) -> AvailablePorts {
    AvailablePorts::new(TestIdentifier::ClassManagerUnitTests.into(), instance_index)
}

pub struct FsClassStorageBuilderForTesting {
    config: FsClassStorageConfig,
    handles: Option<FileHandles>,
}

impl Default for FsClassStorageBuilderForTesting {
    fn default() -> Self {
        let class_hash_storage_handle = tempfile::tempdir().unwrap();
        let persistent_root_handle = tempfile::tempdir().unwrap();
        let persistent_root = persistent_root_handle.path().to_path_buf();
        let mut config = FsClassStorageConfig { persistent_root, ..Default::default() };
        config.class_hash_storage_config.db_config = DbConfig {
            path_prefix: class_hash_storage_handle.path().to_path_buf(),
            max_size: 1 << 30,    // 1GB.
            min_size: 1 << 10,    // 1KB.
            growth_step: 1 << 26, // 64MB.
            ..Default::default()
        };
        Self { config, handles: Some((class_hash_storage_handle, persistent_root_handle)) }
    }
}

impl FsClassStorageBuilderForTesting {
    pub fn with_existing_paths(
        mut self,
        class_hash_storage_path_prefix: PathBuf,
        persistent_path: PathBuf,
    ) -> Self {
        self.config.class_hash_storage_config.db_config.path_prefix =
            class_hash_storage_path_prefix;
        self.config.persistent_root = persistent_path;
        self.handles = None;
        self
    }

    pub fn with_chain_id(mut self, chain_id: ChainId) -> Self {
        self.config.class_hash_storage_config.db_config.chain_id = chain_id;
        self
    }

    pub fn build(
        self,
        instance_index: u16,
    ) -> (FsClassStorage, FsClassStorageConfig, Option<FileHandles>) {
        let Self { config, handles } = self;
        let mut available_ports = available_ports_factory(instance_index);
        let server_config = ServerConfig::new(
            IpAddr::from(Ipv4Addr::LOCALHOST),
            available_ports.get_next_port(),
            false,
        );
        let class_hash_storage = ClassHashStorage::new(
            config.class_hash_storage_config.clone(),
            server_config,
            test_provider(false),
        )
        .unwrap();
        let fs_class_storage =
            FsClassStorage { persistent_root: config.persistent_root.clone(), class_hash_storage };
        (fs_class_storage, config, handles)
    }
}
