pub mod transaction_converter;

use std::error::Error;
use std::sync::Arc;

use apollo_compile_to_casm_types::SierraCompilerError;
use apollo_infra::component_client::{ClientError, LocalComponentClient, RemoteComponentClient};
use apollo_infra::component_definitions::{ComponentClient, ComponentRequestAndResponseSender};
use apollo_infra::impl_debug_for_infra_requests_and_responses;
use apollo_proc_macros::handle_all_response_variants;
use async_trait::async_trait;
#[cfg(feature = "testing")]
use mockall::automock;
use serde::{Deserialize, Serialize};
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedClass;
use starknet_api::state::SierraContractClass;
use strum_macros::AsRefStr;
use thiserror::Error;

pub type ClassManagerResult<T> = Result<T, ClassManagerError>;
pub type ClassManagerClientResult<T> = Result<T, ClassManagerClientError>;

pub type LocalClassManagerClient = LocalComponentClient<ClassManagerRequest, ClassManagerResponse>;
pub type RemoteClassManagerClient =
    RemoteComponentClient<ClassManagerRequest, ClassManagerResponse>;

pub type SharedClassManagerClient = Arc<dyn ClassManagerClient>;
pub type ClassManagerRequestAndResponseSender =
    ComponentRequestAndResponseSender<ClassManagerRequest, ClassManagerResponse>;

// TODO(Elin): export.
pub type ClassId = ClassHash;
pub type Class = SierraContractClass;
pub type ExecutableClass = ContractClass;
pub type ExecutableClassHash = CompiledClassHash;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassHashes {
    pub class_hash: ClassHash,
    pub executable_class_hash: ExecutableClassHash,
}

/// Serves as the class manager's shared interface.
/// Requires `Send + Sync` to allow transferring and sharing resources (inputs, futures) across
/// threads.
#[cfg_attr(feature = "testing", automock)]
#[async_trait]
pub trait ClassManagerClient: Send + Sync {
    async fn add_class(&self, class: Class) -> ClassManagerClientResult<ClassHashes>;

    // TODO(Elin): separate V0 and V1 APIs; remove Sierra version.
    async fn get_executable(
        &self,
        class_id: ClassId,
    ) -> ClassManagerClientResult<Option<ExecutableClass>>;

    async fn get_sierra(&self, class_id: ClassId) -> ClassManagerClientResult<Option<Class>>;

    async fn add_deprecated_class(
        &self,
        class_id: ClassId,
        class: DeprecatedClass,
    ) -> ClassManagerClientResult<()>;

    // This method should only be used through state sync.
    // It acts as a writer to the class storage, and bypasses compilation - thus unsafe.
    async fn add_class_and_executable_unsafe(
        &self,
        class_id: ClassId,
        class: Class,
        executable_class_id: ExecutableClassHash,
        executable_class: ExecutableClass,
    ) -> ClassManagerClientResult<()>;

    // Adds the class without the executable. You will not be able to call `get_executable` or
    // `get_sierra` until the executable is added via `add_missing_executable`.
    // async fn add_class_without_executable
}

#[derive(Clone, Debug, Error, Eq, PartialEq, Serialize, Deserialize)]
pub enum CachedClassStorageError<E: Error> {
    // TODO(Elin): remove from, it's too permissive.
    #[error(transparent)]
    Storage(#[from] E),
}

#[derive(Clone, Debug, Error, Eq, PartialEq, Serialize, Deserialize)]
pub enum ClassManagerError {
    #[error("Internal client error: {0}")]
    Client(String),
    #[error("Failed to deserialize Sierra class: {0}")]
    ClassSerde(String),
    #[error("Class storage error: {0}")]
    ClassStorage(String),
    #[error("Sierra compiler error for class hash {class_hash}: {error}")]
    SierraCompiler {
        class_hash: ClassHash,
        #[source]
        error: SierraCompilerError,
    },
    #[error(
        "Cannot declare contract class with size of {contract_class_object_size}; max allowed \
         size: {max_contract_class_object_size}."
    )]
    ContractClassObjectSizeTooLarge {
        contract_class_object_size: usize,
        max_contract_class_object_size: usize,
    },
}

impl<E: Error> From<CachedClassStorageError<E>> for ClassManagerError {
    fn from(error: CachedClassStorageError<E>) -> Self {
        ClassManagerError::ClassStorage(error.to_string())
    }
}

impl From<serde_json::Error> for ClassManagerError {
    fn from(error: serde_json::Error) -> Self {
        ClassManagerError::ClassSerde(error.to_string())
    }
}

#[derive(Clone, Debug, Error)]
pub enum ClassManagerClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    ClassManagerError(#[from] ClassManagerError),
}

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum ClassManagerRequest {
    AddClass(Class),
    AddClassAndExecutableUnsafe(ClassId, Class, ExecutableClassHash, ExecutableClass),
    AddDeprecatedClass(ClassId, DeprecatedClass),
    GetExecutable(ClassId),
    GetSierra(ClassId),
}
impl_debug_for_infra_requests_and_responses!(ClassManagerRequest);

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum ClassManagerResponse {
    AddClass(ClassManagerResult<ClassHashes>),
    AddClassAndExecutableUnsafe(ClassManagerResult<()>),
    AddDeprecatedClass(ClassManagerResult<()>),
    GetExecutable(ClassManagerResult<Option<ExecutableClass>>),
    GetSierra(ClassManagerResult<Option<Class>>),
}
impl_debug_for_infra_requests_and_responses!(ClassManagerResponse);

#[async_trait]
impl<ComponentClientType> ClassManagerClient for ComponentClientType
where
    ComponentClientType: Send + Sync + ComponentClient<ClassManagerRequest, ClassManagerResponse>,
{
    async fn add_class(&self, class: Class) -> ClassManagerClientResult<ClassHashes> {
        let request = ClassManagerRequest::AddClass(class);
        handle_all_response_variants!(
            ClassManagerResponse,
            AddClass,
            ClassManagerClientError,
            ClassManagerError,
            Direct
        )
    }

    async fn add_deprecated_class(
        &self,
        class_id: ClassId,
        class: DeprecatedClass,
    ) -> ClassManagerClientResult<()> {
        let request = ClassManagerRequest::AddDeprecatedClass(class_id, class);
        handle_all_response_variants!(
            ClassManagerResponse,
            AddDeprecatedClass,
            ClassManagerClientError,
            ClassManagerError,
            Direct
        )
    }

    async fn get_executable(
        &self,
        class_id: ClassId,
    ) -> ClassManagerClientResult<Option<ExecutableClass>> {
        let request = ClassManagerRequest::GetExecutable(class_id);
        handle_all_response_variants!(
            ClassManagerResponse,
            GetExecutable,
            ClassManagerClientError,
            ClassManagerError,
            Direct
        )
    }

    async fn get_sierra(&self, class_id: ClassId) -> ClassManagerClientResult<Option<Class>> {
        let request = ClassManagerRequest::GetSierra(class_id);
        handle_all_response_variants!(
            ClassManagerResponse,
            GetSierra,
            ClassManagerClientError,
            ClassManagerError,
            Direct
        )
    }

    async fn add_class_and_executable_unsafe(
        &self,
        class_id: ClassId,
        class: Class,
        executable_class_id: ExecutableClassHash,
        executable_class: ExecutableClass,
    ) -> ClassManagerClientResult<()> {
        let request = ClassManagerRequest::AddClassAndExecutableUnsafe(
            class_id,
            class,
            executable_class_id,
            executable_class,
        );
        handle_all_response_variants!(
            ClassManagerResponse,
            AddClassAndExecutableUnsafe,
            ClassManagerClientError,
            ClassManagerError,
            Direct
        )
    }
}

pub struct EmptyClassManagerClient;

#[async_trait]
impl ClassManagerClient for EmptyClassManagerClient {
    async fn add_class(&self, _class: Class) -> ClassManagerClientResult<ClassHashes> {
        Ok(Default::default())
    }

    async fn add_deprecated_class(
        &self,
        _class_id: ClassId,
        _class: DeprecatedClass,
    ) -> ClassManagerClientResult<()> {
        Ok(())
    }

    async fn get_executable(
        &self,
        _class_id: ClassId,
    ) -> ClassManagerClientResult<Option<ExecutableClass>> {
        Ok(Some(ExecutableClass::V0(Default::default())))
    }

    async fn get_sierra(&self, _class_id: ClassId) -> ClassManagerClientResult<Option<Class>> {
        Ok(Some(Default::default()))
    }

    async fn add_class_and_executable_unsafe(
        &self,
        _class_id: ClassId,
        _class: Class,
        _executable_class_id: ExecutableClassHash,
        _executable_class: ExecutableClass,
    ) -> ClassManagerClientResult<()> {
        Ok(())
    }
}
