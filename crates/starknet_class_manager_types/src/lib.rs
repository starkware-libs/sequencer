pub mod transaction_converter;

use std::sync::Arc;

use async_trait::async_trait;
use papyrus_proc_macros::handle_all_response_variants;
use serde::{Deserialize, Serialize};
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_sequencer_infra::component_client::ClientError;
use starknet_sequencer_infra::component_definitions::ComponentClient;
use starknet_sierra_compile_types::{RawClass, SierraCompilerClientError, SierraCompilerError};
use thiserror::Error;

pub type ClassManagerResult<T> = Result<T, ClassManagerError>;
pub type ClassManagerClientResult<T> = Result<T, ClassManagerClientError>;

pub type SharedClassManagerClient = Arc<dyn ClassManagerClient>;

// TODO: export.
pub type ClassId = ClassHash;
pub type ExecutableClassHash = CompiledClassHash;

/// Serves as the class manager's shared interface.
/// Requires `Send + Sync` to allow transferring and sharing resources (inputs, futures) across
/// threads.
#[async_trait]
pub trait ClassManagerClient: Send + Sync {
    // TODO(native): make generic in executable type.
    async fn add_class(
        &self,
        class_id: ClassId,
        class: RawClass,
    ) -> ClassManagerClientResult<ExecutableClassHash>;

    async fn get_executable(&self, class_id: ClassId) -> ClassManagerClientResult<RawClass>;

    async fn get_sierra(&self, class_id: ClassId) -> ClassManagerClientResult<RawClass>;

    async fn add_deprecated_class(
        &self,
        class_id: ClassId,
        class: RawClass,
    ) -> ClassManagerClientResult<()>;
}

#[derive(Clone, Debug, Error, Eq, PartialEq, Serialize, Deserialize)]
pub enum ClassStorageError {
    #[error("Class of hash: {class_id} not found")]
    ClassNotFound { class_id: ClassId },
    #[error("Storage error occurred: {0}")]
    StorageError(String),
}

#[derive(Clone, Debug, Error, Eq, PartialEq, Serialize, Deserialize)]
pub enum ClassManagerError {
    #[error("Internal client error: {0}")]
    ClientError(String),
    #[error(transparent)]
    ClassStorageError(#[from] ClassStorageError),
    #[error(transparent)]
    SierraCompilerError(#[from] SierraCompilerError),
}

impl From<SierraCompilerClientError> for ClassManagerError {
    fn from(error: SierraCompilerClientError) -> Self {
        match error {
            SierraCompilerClientError::SierraCompilerError(error) => {
                ClassManagerError::SierraCompilerError(error)
            }
            SierraCompilerClientError::ClientError(error) => {
                ClassManagerError::ClientError(error.to_string())
            }
        }
    }
}

#[derive(Clone, Debug, Error)]
pub enum ClassManagerClientError {
    #[error(transparent)]
    ClassManagerError(#[from] ClassManagerError),
    #[error(transparent)]
    ClientError(#[from] ClientError),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClassManagerRequest {
    AddClass(ClassId, RawClass),
    AddDeprecatedClass(ClassId, RawClass),
    GetExecutable(ClassId),
    GetSierra(ClassId),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClassManagerResponse {
    AddClass(ClassManagerResult<ExecutableClassHash>),
    AddDeprecatedClass(ClassManagerResult<()>),
    GetExecutable(ClassManagerResult<RawClass>),
    GetSierra(ClassManagerResult<RawClass>),
}

#[async_trait]
impl<ComponentClientType> ClassManagerClient for ComponentClientType
where
    ComponentClientType: Send + Sync + ComponentClient<ClassManagerRequest, ClassManagerResponse>,
{
    async fn add_class(
        &self,
        class_id: ClassId,
        class: RawClass,
    ) -> ClassManagerClientResult<ExecutableClassHash> {
        let request = ClassManagerRequest::AddClass(class_id, class);
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
        class: RawClass,
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

    async fn get_executable(&self, class_id: ClassId) -> ClassManagerClientResult<RawClass> {
        let request = ClassManagerRequest::GetExecutable(class_id);
        handle_all_response_variants!(
            ClassManagerResponse,
            GetExecutable,
            ClassManagerClientError,
            ClassManagerError,
            Direct
        )
    }

    async fn get_sierra(&self, class_id: ClassId) -> ClassManagerClientResult<RawClass> {
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
