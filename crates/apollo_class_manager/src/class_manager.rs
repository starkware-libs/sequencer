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
use async_trait::async_trait;
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

    #[instrument(skip(self, class), ret, err)]
    pub async fn add_class(&mut self, class: RawClass) -> ClassManagerResult<ClassHashes> {
        let sierra_class = class.deserialize()?;
        let class_hash = sierra_class.calculate_class_hash();
        if let Ok(Some(executable_class_hash)) = self.classes.get_executable_class_hash(class_hash)
        {
            // Class already exists.
            return Ok(ClassHashes { class_hash, executable_class_hash });
        }

        let (raw_executable_class, executable_class_hash) =
            self.compiler.compile(class).await.map_err(|err| match err {
                SierraCompilerClientError::SierraCompilerError(error) => {
                    ClassManagerError::SierraCompiler { class_hash, error }
                }
                SierraCompilerClientError::ClientError(error) => {
                    ClassManagerError::Client(error.to_string())
                }
            })?;

        self.validate_class_length(&raw_executable_class)?;
        Self::validate_class_version(&sierra_class)?;
        self.classes.set_class(
            class_hash,
            RawClass::try_from(sierra_class)?,
            executable_class_hash,
            raw_executable_class,
        )?;

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
    pub fn get_casm_v1(&self, class_id: ClassId) -> ClassManagerResult<Option<apollo_compile_to_casm_types::RawCasmContractClass>> {
        use apollo_compile_to_casm_types::RawCasmContractClass;
        // Try V1 path by checking existence of executable class hash
        if self.classes.get_executable_class_hash(class_id)?.is_none() {
            return Ok(None);
        }

        let Some(raw_executable) = self.classes.get_executable(class_id)? else {
            return Ok(None);
        };
        // Deserialize to full enum then extract V1 CASM
        let contract_class: starknet_api::contract_class::ContractClass = raw_executable.deserialize()?;
        if let starknet_api::contract_class::ContractClass::V1((casm, _sierra_version)) = contract_class {
            Ok(Some(RawCasmContractClass::try_from(casm)?))
        } else {
            Ok(None)
        }
    }

    #[instrument(skip(self), err)]
    pub fn get_deprecated_executable(&self, class_id: ClassId) -> ClassManagerResult<Option<apollo_compile_to_casm_types::RawDeprecatedExecutableClass>> {
        use apollo_compile_to_casm_types::RawDeprecatedExecutableClass;
        // If a V1 executable class exists, we should not return deprecated here
        if self.classes.get_executable_class_hash(class_id)?.is_some() {
            return Ok(None);
        }

        // Try deprecated class directly
        if let Some(raw_deprecated) = self.classes.get_deprecated_class(class_id)? {
            let contract_class: starknet_api::contract_class::ContractClass =
                raw_deprecated.deserialize()?;
            if let starknet_api::contract_class::ContractClass::V0(depr) = contract_class {
                return Ok(Some(
                    apollo_compile_to_casm_types::RawDeprecatedExecutableClass::try_from(depr)?,
                ));
            }
        }

        // As a fallback, if executable() returns V0, convert
        if let Some(raw_exec) = self.classes.get_executable(class_id)? {
            let contract_class: starknet_api::contract_class::ContractClass = raw_exec.deserialize()?;
            if let starknet_api::contract_class::ContractClass::V0(depr) = contract_class {
                return Ok(Some(RawDeprecatedExecutableClass::try_from(depr)?));
            }
        }
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
) -> FsClassManager {
    let FsClassManagerConfig { class_manager_config, class_storage_config } = config;
    let fs_class_storage =
        FsClassStorage::new(class_storage_config).expect("Failed to create class storage.");
    let class_manager = ClassManager::new(class_manager_config, compiler_client, fs_class_storage);

    FsClassManager(class_manager)
}

#[async_trait]
impl ComponentStarter for FsClassManager {
    async fn start(&mut self) {
        default_component_start_fn::<Self>().await;
        register_metrics();
    }
}
