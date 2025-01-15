use starknet_class_manager_types::{ClassId, ClassManagerResult, ExecutableClassHash};
use starknet_sierra_compile_types::{RawClass, RawExecutableClass, SharedSierraCompilerClient};

use crate::class_storage::{CachedClassStorage, CachedClassStorageConfig, ClassStorage};
pub mod class_storage;

#[derive(Clone, Copy, Debug)]
struct ClassManagerConfig {
    cached_class_storage_config: CachedClassStorageConfig,
}

pub struct ClassManager<S: ClassStorage> {
    config: ClassManagerConfig,
    compiler: SharedSierraCompilerClient,
    classes: CachedClassStorage<S>,
}

impl<S: ClassStorage> ClassManager<S> {
    fn new(config: ClassManagerConfig, compiler: SharedSierraCompilerClient, storage: S) -> Self {
        Self {
            config,
            compiler,
            classes: CachedClassStorage::new(config.cached_class_storage_config, storage),
        }
    }
}

impl<S: ClassStorage> ClassManager<S> {
    async fn add_class(
        &mut self,
        class_id: ClassId,
        class: RawClass,
    ) -> ClassManagerResult<ExecutableClassHash> {
        let (raw_executable_class, executable_class_hash) =
            self.compiler.compile(class.clone()).await?;

        self.classes.set_class(class_id, class, executable_class_hash, raw_executable_class)?;

        Ok(executable_class_hash)
    }

    fn get_executable(&self, class_id: ClassId) -> ClassManagerResult<RawExecutableClass> {
        Ok(self.classes.get_executable(class_id)?)
    }

    fn get_sierra(&self, class_id: ClassId) -> ClassManagerResult<RawClass> {
        Ok(self.classes.get_sierra(class_id)?)
    }

    fn add_deprecated_class(
        &mut self,
        class_id: ClassId,
        class: RawExecutableClass,
    ) -> ClassManagerResult<()> {
        self.classes.set_deprecated_class(class_id, class)?;
        Ok(())
    }
}
