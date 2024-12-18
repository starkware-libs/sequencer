use async_trait::async_trait;
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use papyrus_proc_macros::handle_response_variants;
use serde::{Deserialize, Serialize};
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::state::SierraContractClass;
use starknet_sequencer_infra::component_client::ClientError;
use starknet_sequencer_infra::component_definitions::ComponentClient;
use thiserror::Error;

pub type ClassManagerResult<T> = Result<T, ClassManagerError>;
pub type ClassManagerClientResult<T> = Result<T, ClassManagerClientError>;

// TODO: export.
pub type ClassId = ClassHash;
pub type Class = SierraContractClass;
pub type ExecutableClass = CasmContractClass;
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
        class: Class,
    ) -> ClassManagerClientResult<ExecutableClassHash>;

    async fn get_executable(&self, class_id: ClassId) -> ClassManagerClientResult<ExecutableClass>;

    async fn get_sierra(&self, class_id: ClassId) -> ClassManagerClientResult<Class>;
}

#[derive(Clone, Debug, Error, Eq, PartialEq, Serialize, Deserialize)]
pub enum ClassManagerError {
    #[error("Compilation failed: {0}")]
    CompilationUtilError(String),
    #[error("Class of hash: {class_id} not found")]
    ClassNotFound { class_id: ClassId },
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
    AddClass(ClassId, Class),
    GetExecutable(ClassId),
    GetSierra(ClassId),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ClassManagerResponse {
    AddClass(ClassManagerResult<ExecutableClassHash>),
    GetExecutable(ClassManagerResult<ExecutableClass>),
    GetSierra(ClassManagerResult<Class>),
}

#[async_trait]
impl<ComponentClientType> ClassManagerClient for ComponentClientType
where
    ComponentClientType: Send + Sync + ComponentClient<ClassManagerRequest, ClassManagerResponse>,
{
    async fn add_class(
        &self,
        class_id: ClassId,
        class: Class,
    ) -> ClassManagerClientResult<ExecutableClassHash> {
        let request = ClassManagerRequest::AddClass(class_id, class);
        let response = self.send(request).await;
        handle_response_variants!(
            ClassManagerResponse,
            AddClass,
            ClassManagerClientError,
            ClassManagerError
        )
    }

    async fn get_executable(&self, class_id: ClassId) -> ClassManagerClientResult<ExecutableClass> {
        let request = ClassManagerRequest::GetExecutable(class_id);
        let response = self.send(request).await;
        handle_response_variants!(
            ClassManagerResponse,
            GetExecutable,
            ClassManagerClientError,
            ClassManagerError
        )
    }

    async fn get_sierra(&self, class_id: ClassId) -> ClassManagerClientResult<Class> {
        let request = ClassManagerRequest::GetSierra(class_id);
        let response = self.send(request).await;
        handle_response_variants!(
            ClassManagerResponse,
            GetSierra,
            ClassManagerClientError,
            ClassManagerError
        )
    }
}
