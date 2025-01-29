use starknet_api::state::SierraContractClass;
use starknet_class_manager_types::{ClassHashes, ClassId, ClassManagerError, ClassManagerResult};
use starknet_sierra_multicompile_types::{
    RawClass,
    RawExecutableClass,
    SharedSierraCompilerClient,
};

use crate::class_storage::{CachedClassStorage, CachedClassStorageConfig, ClassStorage};

#[cfg(test)]
#[path = "class_manager_test.rs"]
pub mod class_manager_test;

#[derive(Clone, Copy, Debug)]
pub struct ClassManagerConfig {
    pub cached_class_storage_config: CachedClassStorageConfig,
}

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
        Self {
            config,
            compiler,
            classes: CachedClassStorage::new(config.cached_class_storage_config, storage),
        }
    }
}

impl<S: ClassStorage> ClassManager<S> {
    pub async fn add_class(&mut self, class: RawClass) -> ClassManagerResult<ClassHashes> {
        // TODO(Elin): think how to not clone the class to deserialize.
        let sierra_class =
            SierraContractClass::try_from(class.clone()).map_err(ClassManagerError::from)?;
        let class_hash = sierra_class.calculate_class_hash();
        if let Ok(executable_class_hash) = self.classes.get_executable_class_hash(class_hash) {
            // Class already exists.
            let class_hashes = ClassHashes { class_hash, executable_class_hash };
            return Ok(class_hashes);
        }

        let (raw_executable_class, executable_class_hash) =
            self.compiler.compile(class.clone()).await?;

        self.classes.set_class(class_hash, class, executable_class_hash, raw_executable_class)?;

        let class_hashes = ClassHashes { class_hash, executable_class_hash };
        Ok(class_hashes)
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
