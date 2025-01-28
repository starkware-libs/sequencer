use starknet_class_manager_types::{ClassId, ClassManagerResult, ExecutableClassHash};
#[cfg(test)]
use starknet_sierra_multicompile_types::MockSierraCompilerClient;
use starknet_sierra_multicompile_types::{
    RawClass,
    RawExecutableClass,
    SharedSierraCompilerClient,
};

#[cfg(test)]
use crate::class_storage::FsClassStorage;
use crate::class_storage::{CachedClassStorage, CachedClassStorageConfig, ClassStorage};

#[cfg(test)]
#[path = "class_manager_test.rs"]
pub mod class_manager_test;

#[derive(Clone, Copy, Debug)]
pub struct ClassManagerConfig {
    pub cached_class_storage_config: CachedClassStorageConfig,
}

pub struct ClassManager<S: ClassStorage> {
    _config: ClassManagerConfig,
    compiler: SharedSierraCompilerClient,
    classes: CachedClassStorage<S>,
}

impl<S: ClassStorage> ClassManager<S> {
    pub fn new(
        config: ClassManagerConfig,
        compiler: SharedSierraCompilerClient,
        storage: S,
    ) -> Self {
        Self {
            _config: config,
            compiler,
            classes: CachedClassStorage::new(config.cached_class_storage_config, storage),
        }
    }
}

#[cfg(test)]
impl ClassManager<FsClassStorage> {
    fn new_for_testing(compiler: MockSierraCompilerClient) -> Self {
        use std::sync::Arc;

        use crate::class_storage::FsClassStorage;

        let cached_class_storage_config =
            CachedClassStorageConfig { class_cache_size: 10, deprecated_class_cache_size: 10 };
        let config = ClassManagerConfig { cached_class_storage_config };
        let storage = FsClassStorage::new_for_testing();

        ClassManager::new(config, Arc::new(compiler), storage)
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
