use starknet_api::state::SierraContractClass;
use starknet_class_manager_types::{ClassHashes, ClassId, ClassManagerError, ClassManagerResult};
use starknet_sequencer_infra::component_definitions::ComponentStarter;
use starknet_sierra_multicompile_types::{
    RawClass,
    RawExecutableClass,
    SharedSierraCompilerClient,
};

use crate::class_storage::{CachedClassStorage, ClassStorage, FsClassStorage};
use crate::config::ClassManagerConfig;
use crate::FsClassManager;

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

// TODO(Elin): properly propagate result from storage creation.
pub fn create_class_manager(
    config: ClassManagerConfig,
    compiler_client: SharedSierraCompilerClient,
) -> FsClassManager {
    let fs_class_storage = FsClassStorage::new(config.class_storage_config.clone())
        .expect("Failed to build class storage");
    let class_manager = ClassManager::new(config, compiler_client, fs_class_storage);
    FsClassManager(class_manager)
}

impl ComponentStarter for FsClassManager {}
