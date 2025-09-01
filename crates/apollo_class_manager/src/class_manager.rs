use apollo_class_manager_types::{
    ClassHashes,
    ClassId,
    ClassManagerError,
    ClassManagerResult,
    ExecutableClassHash,
};
use apollo_compile_to_casm_types::{
    RawClass,
    RawExecutableClass,
    SharedSierraCompilerClient,
    SierraCompilerClientError,
};
use apollo_infra::component_definitions::{default_component_start_fn, ComponentStarter};
use apollo_state_sync_types::communication::SharedStateSyncClient;
use async_trait::async_trait;
use starknet_api::contract_class::ContractClass;
use starknet_api::state::{SierraContractClass, CONTRACT_CLASS_VERSION};
use tracing::instrument;

use crate::class_storage::{CachedClassStorage, ClassStorage, FsClassStorage};
use crate::config::{ClassManagerConfig, FsClassManagerConfig};
use crate::metrics::register_metrics;
use crate::FsClassManager;

#[cfg(test)]
#[path = "class_manager_test.rs"]
pub mod class_manager_test;

pub struct ClassManager<S: ClassStorage> {
    pub config: ClassManagerConfig,
    pub compiler: SharedSierraCompilerClient,
    pub classes: CachedClassStorage<S>,
    pub state_sync_client: Option<SharedStateSyncClient>,
}

impl<S: ClassStorage> ClassManager<S> {
    pub fn new(
        config: ClassManagerConfig,
        compiler: SharedSierraCompilerClient,
        storage: S,
        state_sync_client: Option<SharedStateSyncClient>,
    ) -> Self {
        let cached_class_storage_config = config.cached_class_storage_config.clone();
        Self {
            config,
            compiler,
            classes: CachedClassStorage::new(cached_class_storage_config, storage),
            state_sync_client,
        }
    }

    #[instrument(skip(self, class), ret, err)]
    pub async fn add_class(&mut self, class: RawClass) -> ClassManagerResult<ClassHashes> {
        // TODO(Elin): think how to not clone the class to deserialize.
        let sierra_class = SierraContractClass::try_from(class.clone())?;
        let class_hash = sierra_class.calculate_class_hash();
        if let Ok(Some(executable_class_hash)) = self.classes.get_executable_class_hash(class_hash)
        {
            // Class already exists.
            return Ok(ClassHashes { class_hash, executable_class_hash });
        }

        let (raw_executable_class, executable_class_hash) =
            self.compiler.compile(class.clone()).await.map_err(|err| match err {
                SierraCompilerClientError::SierraCompilerError(error) => {
                    ClassManagerError::SierraCompiler { class_hash, error }
                }
                SierraCompilerClientError::ClientError(error) => {
                    ClassManagerError::Client(error.to_string())
                }
            })?;

        self.validate_class_length(&raw_executable_class)?;
        Self::validate_class_version(&sierra_class)?;
        self.classes.set_class(class_hash, class, executable_class_hash, raw_executable_class)?;

        let class_hashes = ClassHashes { class_hash, executable_class_hash };
        Ok(class_hashes)
    }

    #[instrument(skip(self), err)]
    pub async fn get_executable(
        &self,
        class_id: ClassId,
    ) -> ClassManagerResult<Option<RawExecutableClass>> {
        // First try to get the class from local storage
        if let Ok(Some(executable_class)) = self.classes.get_executable(class_id) {
            return Ok(Some(executable_class));
        }

        // If not found locally and we have a state sync client, try to get from state sync
        if let Some(state_sync_client) = &self.state_sync_client {
            match state_sync_client.get_deprecated_class(class_id).await {
                Ok(Some(deprecated_class)) => {
                    // Convert DeprecatedContractClass to RawExecutableClass
                    let contract_class = ContractClass::V0(deprecated_class);
                    let raw_executable_class = RawExecutableClass::try_from(contract_class)
                        .map_err(|e| ClassManagerError::ClassSerde(e.to_string()))?;
                    return Ok(Some(raw_executable_class));
                }
                Ok(None) => {
                    // Class not found in state sync either
                    return Ok(None);
                }
                Err(e) => {
                    // Ignore errors from state sync and return None
                    // We don't want state sync failures to break the class manager
                    tracing::warn!("Error getting deprecated class from state sync: {:?}", e);
                    return Ok(None);
                }
            }
        }

        // If no state sync client or class not found, return None
        Ok(None)
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

    #[instrument(skip(self, class, executable_class), ret, err)]
    pub fn add_class_and_executable_unsafe(
        &mut self,
        class_id: ClassId,
        class: RawClass,
        executable_class_id: ExecutableClassHash,
        executable_class: RawExecutableClass,
    ) -> ClassManagerResult<()> {
        Ok(self.classes.set_class(class_id, class, executable_class_id, executable_class)?)
    }

    fn validate_class_length(
        &self,
        serialized_class: &RawExecutableClass,
    ) -> ClassManagerResult<()> {
        // Note: The class bytecode length is validated in the compiler.

        let contract_class_object_size =
            serialized_class.size().expect("Unexpected error serializing contract class.");
        if contract_class_object_size > self.config.max_compiled_contract_class_object_size {
            return Err(ClassManagerError::ContractClassObjectSizeTooLarge {
                contract_class_object_size,
                max_contract_class_object_size: self.config.max_compiled_contract_class_object_size,
            });
        }

        Ok(())
    }

    /// Validates the version of the class.
    fn validate_class_version(sierra: &SierraContractClass) -> ClassManagerResult<()> {
        if sierra.contract_class_version != CONTRACT_CLASS_VERSION {
            return Err(ClassManagerError::UnsupportedContractClassVersion(
                sierra.contract_class_version.to_string(),
            ));
        }
        Ok(())
    }
}

pub fn create_class_manager(
    config: FsClassManagerConfig,
    compiler_client: SharedSierraCompilerClient,
    state_sync_client: Option<SharedStateSyncClient>,
) -> FsClassManager {
    let FsClassManagerConfig { class_manager_config, class_storage_config } = config;
    let fs_class_storage =
        FsClassStorage::new(class_storage_config).expect("Failed to create class storage.");
    let class_manager = ClassManager::new(
        class_manager_config,
        compiler_client,
        fs_class_storage,
        state_sync_client,
    );

    FsClassManager(class_manager)
}

#[async_trait]
impl ComponentStarter for FsClassManager {
    async fn start(&mut self) {
        default_component_start_fn::<Self>().await;
        register_metrics();
    }
}
