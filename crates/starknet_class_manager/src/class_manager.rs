use async_trait::async_trait;
use starknet_api::state::SierraContractClass;
use starknet_class_manager_types::{ClassHashes, ClassId, ClassManagerError, ClassManagerResult};
use starknet_sequencer_infra::component_definitions::{
    default_component_start_fn,
    ComponentStarter,
};
use starknet_sequencer_metrics::metric_definitions::N_CLASSES;
use starknet_sierra_multicompile_types::{
    RawClass,
    RawExecutableClass,
    SharedSierraCompilerClient,
};
use strum::IntoEnumIterator;
use tracing::instrument;

use crate::class_storage::{CachedClassStorage, ClassStorage, FsClassStorage};
use crate::config::{ClassManagerConfig, FsClassManagerConfig};
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
        register_metrics();

        let cached_class_storage_config = config.cached_class_storage_config.clone();
        Self {
            config,
            compiler,
            classes: CachedClassStorage::new(cached_class_storage_config, storage),
        }
    }
}

impl<S: ClassStorage> ClassManager<S> {
    #[instrument(skip(self, class), ret, err)]
    pub async fn add_class(&mut self, class: RawClass) -> ClassManagerResult<ClassHashes> {
        // TODO(Elin): think how to not clone the class to deserialize.
        let sierra_class =
            SierraContractClass::try_from(class.clone()).map_err(ClassManagerError::from)?;
        let class_hash = sierra_class.calculate_class_hash();
        if let Ok(Some(executable_class_hash)) = self.classes.get_executable_class_hash(class_hash)
        {
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

    #[instrument(skip(self), err)]
    pub fn get_executable(
        &self,
        class_id: ClassId,
    ) -> ClassManagerResult<Option<RawExecutableClass>> {
        Ok(self.classes.get_executable(class_id)?)
    }

    #[instrument(skip(self), err)]
    pub fn get_sierra(&self, class_id: ClassId) -> ClassManagerResult<Option<RawClass>> {
        Ok(self.classes.get_sierra(class_id)?)
    }

    #[instrument(skip(self, class), ret, err)]
    pub fn add_deprecated_class(
        &mut self,
        class_id: ClassId,
        class: RawExecutableClass,
    ) -> ClassManagerResult<()> {
        self.classes.set_deprecated_class(class_id, class)?;
        Ok(())
    }
}

pub fn create_class_manager(
    config: FsClassManagerConfig,
    compiler_client: SharedSierraCompilerClient,
) -> FsClassManager {
    let FsClassManagerConfig { class_manager_config, class_storage_config } = config;
    let fs_class_storage =
        FsClassStorage::new(class_storage_config).expect("Failed to create class storage.");
    let class_manager = ClassManager::new(class_manager_config, compiler_client, fs_class_storage);

    FsClassManager(class_manager)
}

const CLASS_TYPE_LABEL: &str = "class_type";

#[derive(strum_macros::IntoStaticStr, strum_macros::EnumIter)]
#[strum(serialize_all = "snake_case")]
enum ClassType {
    Regular,
    Deprecated,
}

fn register_metrics() {
    let labels: Vec<(&str, &str)> =
        ClassType::iter().map(|cls_type| (CLASS_TYPE_LABEL, cls_type.into())).collect();

    N_CLASSES.register(&[labels]);
}

#[async_trait]
impl ComponentStarter for FsClassManager {
    async fn start(&mut self) {
        default_component_start_fn::<Self>().await;
        register_metrics();
    }
}
