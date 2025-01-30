use starknet_class_manager_types::{ClassId, ClassManagerResult, ExecutableClassHash};
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use starknet_sierra_multicompile_types::{
    RawClass,
    RawExecutableClass,
    SharedSierraCompilerClient,
};

use crate::class_storage::{CachedClassStorage, ClassHashStorage, ClassStorage, FsClassStorage};
use crate::config::{ClassHashStorageConfig, ClassManagerConfig};

#[cfg(test)]
#[path = "class_manager_test.rs"]
pub mod class_manager_test;

pub struct ClassManager<S: ClassStorage> {
    pub config: ClassManagerConfig,
    pub compiler: SharedSierraCompilerClient,
    pub classes: CachedClassStorage<S>,
}

impl<S: ClassStorage> ClassManager<S> {
    pub fn new(
        config: ClassManagerConfig,
        compiler: SharedSierraCompilerClient,
        storage: S,
    ) -> Self {
        let cached_class_storage_config = config.cached_class_storage_config.clone();
        Self {
            config,
            compiler,
            classes: CachedClassStorage::new(cached_class_storage_config, storage),
        }
    }
}

impl<S: ClassStorage> ClassManager<S> {
    pub async fn add_class(
        &mut self,
        class_id: ClassId,
        class: RawClass,
    ) -> ClassManagerResult<ExecutableClassHash> {
        if let Ok(executable_class_hash) = self.classes.get_executable_class_hash(class_id) {
            // Class already exists.
            return Ok(executable_class_hash);
        }

        let (raw_executable_class, executable_class_hash) =
            self.compiler.compile(class.clone()).await?;

        self.classes.set_class(class_id, class, executable_class_hash, raw_executable_class)?;

        Ok(executable_class_hash)
    }

    pub fn get_executable(&self, class_id: ClassId) -> ClassManagerResult<RawExecutableClass> {
        Ok(self.classes.get_executable(class_id)?)
    }

    pub fn get_sierra(&self, class_id: ClassId) -> ClassManagerResult<RawClass> {
        Ok(self.classes.get_sierra(class_id)?)
    }

    pub fn add_deprecated_class(
        &mut self,
        class_id: ClassId,
        class: RawExecutableClass,
    ) -> ClassManagerResult<()> {
        self.classes.set_deprecated_class(class_id, class)?;
        Ok(())
    }
}

// TODO(Elin): rewrite this function
pub fn create_class_manager(
    config: ClassManagerConfig,
    compiler_client: SharedSierraCompilerClient,
) -> ClassManager<FsClassStorage> {
    let persistent_root = tempfile::tempdir().unwrap().path().to_path_buf();
    let storage_config = ClassHashStorageConfig {
        path_prefix: tempfile::tempdir().unwrap().path().to_path_buf(),
        ..Default::default()
    };
    let class_hash_storage = ClassHashStorage::new(storage_config).unwrap();
    let fs_class_storage = FsClassStorage::new(persistent_root, class_hash_storage);

    ClassManager::new(config, compiler_client, fs_class_storage)
}

impl ComponentStarter for ClassManager<FsClassStorage> {}
