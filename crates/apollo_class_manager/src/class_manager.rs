use std::time::Instant;

use apollo_class_manager_config::config::{ClassManagerDynamicConfig, FsClassManagerConfig};
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
use apollo_config_manager_types::communication::SharedConfigManagerClient;
use apollo_infra::component_definitions::{default_component_start_fn, ComponentStarter};
use async_trait::async_trait;
use starknet_api::state::{SierraContractClass, CONTRACT_CLASS_VERSION};
use tracing::{debug, instrument};

use crate::class_storage::{CachedClassStorage, ClassStorage, FsClassStorage};
use crate::metrics::register_metrics;
use crate::FsClassManager;

#[cfg(test)]
#[path = "class_manager_test.rs"]
pub mod class_manager_test;

pub struct ClassManager<S: ClassStorage> {
    pub config: FsClassManagerConfig,
    pub compiler: SharedSierraCompilerClient,
    pub classes: CachedClassStorage<S>,
    pub config_manager_client: SharedConfigManagerClient,
}

impl<S> ClassManager<S>
where
    S: ClassStorage,
    apollo_class_manager_types::CachedClassStorageError<S::Error>: From<S::Error>,
{
    pub fn new(
        config: FsClassManagerConfig,
        compiler: SharedSierraCompilerClient,
        storage: S,
        config_manager_client: SharedConfigManagerClient,
    ) -> Self {
        let cached_class_storage_config =
            config.static_config.class_manager_config.cached_class_storage_config.clone();
        Self {
            config,
            compiler,
            classes: CachedClassStorage::new(cached_class_storage_config, storage),
            config_manager_client,
        }
    }

    pub fn update_dynamic_config(&mut self, dynamic_config: ClassManagerDynamicConfig) {
        self.config.dynamic_config = dynamic_config;
    }

    #[instrument(skip(self, class), ret, err)]
    pub async fn add_class(&mut self, class: RawClass) -> ClassManagerResult<ClassHashes> {
        let sierra_class = SierraContractClass::try_from(&class)?;
        let class_hash = sierra_class.calculate_class_hash();
        if let Ok(Some(executable_class_hash_v2)) =
            self.classes.get_executable_class_hash_v2(class_hash)
        {
            // Class already exists.
            return Ok(ClassHashes { class_hash, executable_class_hash_v2 });
        }

        let compilation_start_time = Instant::now();
        let (raw_executable_class, executable_class_hash_v2) =
            self.compiler.compile(class.clone()).await.map_err(|err| match err {
                SierraCompilerClientError::SierraCompilerError(error) => {
                    ClassManagerError::SierraCompiler { class_hash, error }
                }
                SierraCompilerClientError::ClientError(error) => {
                    ClassManagerError::Client(error.to_string())
                }
            })?;
        debug!(
            %class_hash,
            compiled_class_hash = %executable_class_hash_v2,
            compilation_elapsed_ms = compilation_start_time.elapsed().as_millis(),
            class_size_bytes = class.size().map(|size| size.to_string()).unwrap_or("Failed to get class size".to_owned()),
            casm_size_bytes = raw_executable_class.size().map(|size| size.to_string()).unwrap_or("Failed to get casm size".to_owned()),
            "Finished compiling class."
        );

        self.validate_class_length(&raw_executable_class)?;
        Self::validate_class_version(&sierra_class)?;
        self.classes.set_class(
            class_hash,
            class,
            executable_class_hash_v2,
            raw_executable_class,
        )?;

        let class_hashes = ClassHashes { class_hash, executable_class_hash_v2 };
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

    #[instrument(skip(self), err)]
    pub fn get_executable_class_hash_v2(
        &self,
        class_id: ClassId,
    ) -> ClassManagerResult<Option<ExecutableClassHash>> {
        Ok(self.classes.get_executable_class_hash_v2(class_id)?)
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
        executable_class_hash_v2: ExecutableClassHash,
        executable_class: RawExecutableClass,
    ) -> ClassManagerResult<()> {
        Ok(self.classes.set_class(class_id, class, executable_class_hash_v2, executable_class)?)
    }

    fn validate_class_length(
        &self,
        serialized_class: &RawExecutableClass,
    ) -> ClassManagerResult<()> {
        // Note: The class bytecode length is validated in the compiler.

        let contract_class_object_size =
            serialized_class.size().expect("Unexpected error serializing contract class.");
        if contract_class_object_size
            > self.config.static_config.class_manager_config.max_compiled_contract_class_object_size
        {
            return Err(ClassManagerError::ContractClassObjectSizeTooLarge {
                contract_class_object_size,
                max_contract_class_object_size: self
                    .config
                    .static_config
                    .class_manager_config
                    .max_compiled_contract_class_object_size,
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
    config_manager_client: SharedConfigManagerClient,
) -> FsClassManager {
    let FsClassManagerConfig { static_config, dynamic_config } = config.clone();
    let fs_class_storage = FsClassStorage::new(
        static_config.class_storage_config,
        dynamic_config.storage_reader_server_dynamic_config,
    )
    .expect("Failed to create class storage.");
    let class_manager =
        ClassManager::new(config, compiler_client, fs_class_storage, config_manager_client);

    FsClassManager(class_manager)
}

#[async_trait]
impl ComponentStarter for FsClassManager {
    async fn start(&mut self) {
        default_component_start_fn::<Self>().await;
        register_metrics();
    }
}
