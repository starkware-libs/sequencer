pub mod transaction_converter;

use std::error::Error;
use std::sync::Arc;

use async_trait::async_trait;
#[cfg(feature = "testing")]
use mockall::automock;
use papyrus_proc_macros::handle_all_response_variants;
use serde::{Deserialize, Serialize};
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedClass;
use starknet_api::state::SierraContractClass;
use starknet_sequencer_infra::component_client::{
    ClientError,
    LocalComponentClient,
    RemoteComponentClient,
};
use starknet_sequencer_infra::component_definitions::{
    ComponentClient,
    ComponentRequestAndResponseSender,
};
use starknet_sierra_multicompile_types::{SierraCompilerClientError, SierraCompilerError};
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

    async fn get_executable(&self, class_id: ClassId) -> ClassManagerClientResult<ExecutableClass>;

    async fn get_sierra(&self, class_id: ClassId) -> ClassManagerClientResult<Class>;

    async fn add_deprecated_class(
        &self,
        class_id: ClassId,
        class: DeprecatedClass,
    ) -> ClassManagerClientResult<()>;
}

#[derive(Clone, Debug, Error, Eq, PartialEq, Serialize, Deserialize)]
pub enum CachedClassStorageError<E: Error> {
    #[error("Class of hash: {class_id} not found")]
    ClassNotFound { class_id: ClassId },
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
    #[error(transparent)]
    SierraCompiler(#[from] SierraCompilerError),
}

impl<E: Error> From<CachedClassStorageError<E>> for ClassManagerError {
    fn from(error: CachedClassStorageError<E>) -> Self {
        ClassManagerError::ClassStorage(error.to_string())
    }
}

impl From<SierraCompilerClientError> for ClassManagerError {
    fn from(error: SierraCompilerClientError) -> Self {
        match error {
            SierraCompilerClientError::SierraCompilerError(error) => {
                ClassManagerError::SierraCompiler(error)
            }
            SierraCompilerClientError::ClientError(error) => {
                ClassManagerError::Client(error.to_string())
            }
        }
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClassManagerRequest {
    AddClass(Class),
    AddDeprecatedClass(ClassId, DeprecatedClass),
    GetExecutable(ClassId),
    GetSierra(ClassId),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClassManagerResponse {
    AddClass(ClassManagerResult<ClassHashes>),
    AddDeprecatedClass(ClassManagerResult<()>),
    GetExecutable(ClassManagerResult<ExecutableClass>),
    GetSierra(ClassManagerResult<Class>),
}

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

    async fn get_executable(&self, class_id: ClassId) -> ClassManagerClientResult<ExecutableClass> {
        let request = ClassManagerRequest::GetExecutable(class_id);
        handle_all_response_variants!(
            ClassManagerResponse,
            GetExecutable,
            ClassManagerClientError,
            ClassManagerError,
            Direct
        )
    }

    async fn get_sierra(&self, class_id: ClassId) -> ClassManagerClientResult<Class> {
        let request = ClassManagerRequest::GetSierra(class_id);
        handle_all_response_variants!(
            ClassManagerResponse,
            GetSierra,
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
    ) -> ClassManagerClientResult<ExecutableClass> {
        Ok(ExecutableClass::V0(Default::default()))
    }

    async fn get_sierra(&self, _class_id: ClassId) -> ClassManagerClientResult<Class> {
        Ok(Default::default())
    }
}
