pub mod transaction_converter;

use std::error::Error;
use std::sync::Arc;

use apollo_compile_to_casm_types::{
    RawCasmContractClass,
    RawClass,
    RawDeprecatedExecutableClass,
    RawExecutableClass,
    SierraCompilerError,
};
use apollo_infra::component_client::{ClientError, LocalComponentClient, RemoteComponentClient};
use apollo_infra::component_definitions::{
    ComponentClient,
    PrioritizedRequest,
    RequestPriority,
    RequestWrapper,
};
use apollo_infra::{impl_debug_for_infra_requests_and_responses, impl_labeled_request};
// use apollo_proc_macros::handle_all_response_variants;
use async_trait::async_trait;
#[cfg(feature = "testing")]
use mockall::automock;
use serde::{Deserialize, Serialize};
use starknet_api::contract_class::ContractClass;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::deprecated_contract_class::ContractClass as DeprecatedClass;
use starknet_api::state::SierraContractClass;
use strum::EnumVariantNames;
use strum_macros::{AsRefStr, EnumDiscriminants, EnumIter, IntoStaticStr};
use thiserror::Error;

pub type ClassManagerResult<T> = Result<T, ClassManagerError>;
pub type ClassManagerClientResult<T> = Result<T, ClassManagerClientError>;

pub type LocalClassManagerClient = LocalComponentClient<ClassManagerRequest, ClassManagerResponse>;
pub type RemoteClassManagerClient =
    RemoteComponentClient<ClassManagerRequest, ClassManagerResponse>;

pub type SharedClassManagerClient = Arc<dyn ClassManagerClient>;
pub type ClassManagerRequestWrapper = RequestWrapper<ClassManagerRequest, ClassManagerResponse>;

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

    // V1: Get CASM only (without Sierra version)
    async fn get_casm_v1(
        &self,
        class_id: ClassId,
    ) -> ClassManagerClientResult<Option<RawCasmContractClass>>;

    // V0: Get deprecated executable class
    async fn get_deprecated_executable(
        &self,
        class_id: ClassId,
    ) -> ClassManagerClientResult<Option<RawDeprecatedExecutableClass>>;

    // Backward-compat helper that reconstructs ContractClass (includes SierraVersion) from the
    // split APIs.
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
}

#[derive(Clone, Debug, Error, Eq, PartialEq, Serialize, Deserialize)]
pub enum CachedClassStorageError<E: Error> {
    // TODO(Elin): remove from, it's too permissive.
    #[error(transparent)]
    Storage(E),
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
    #[error("Unsupported contract class version: {0}.")]
    UnsupportedContractClassVersion(String),
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

#[derive(Serialize, Deserialize, Clone, AsRefStr, EnumDiscriminants)]
#[strum_discriminants(
    name(ClassManagerRequestLabelValue),
    derive(IntoStaticStr, EnumIter, EnumVariantNames),
    strum(serialize_all = "snake_case")
)]
pub enum ClassManagerRequest {
    AddClass(RawClass),
    AddClassAndExecutableUnsafe(ClassId, RawClass, ExecutableClassHash, RawExecutableClass),
    AddDeprecatedClass(ClassId, RawExecutableClass),
    GetExecutable(ClassId),
    GetCasmV1(ClassId),
    GetDeprecatedExecutable(ClassId),
    GetSierra(ClassId),
}
impl_debug_for_infra_requests_and_responses!(ClassManagerRequest);
impl_labeled_request!(ClassManagerRequest, ClassManagerRequestLabelValue);
impl PrioritizedRequest for ClassManagerRequest {
    fn priority(&self) -> RequestPriority {
        match self {
            ClassManagerRequest::GetExecutable(_)
            | ClassManagerRequest::GetCasmV1(_)
            | ClassManagerRequest::GetDeprecatedExecutable(_)
            | ClassManagerRequest::GetSierra(_) => RequestPriority::High,

            ClassManagerRequest::AddClass(_)
            | ClassManagerRequest::AddClassAndExecutableUnsafe(_, _, _, _)
            | ClassManagerRequest::AddDeprecatedClass(_, _) => RequestPriority::Normal,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, AsRefStr)]
pub enum ClassManagerResponse {
    AddClass(ClassManagerResult<ClassHashes>),
    AddClassAndExecutableUnsafe(ClassManagerResult<()>),
    AddDeprecatedClass(ClassManagerResult<()>),
    GetExecutable(ClassManagerResult<Option<RawExecutableClass>>),
    GetCasmV1(ClassManagerResult<Option<RawCasmContractClass>>),
    GetDeprecatedExecutable(ClassManagerResult<Option<RawDeprecatedExecutableClass>>),
    GetSierra(ClassManagerResult<Option<RawClass>>),
}
impl_debug_for_infra_requests_and_responses!(ClassManagerResponse);

#[async_trait]
impl<ComponentClientType> ClassManagerClient for ComponentClientType
where
    ComponentClientType: Send + Sync + ComponentClient<ClassManagerRequest, ClassManagerResponse>,
{
    async fn add_class(&self, class: Class) -> ClassManagerClientResult<ClassHashes> {
        let raw = RawClass::try_from(class).map_err(ClassManagerError::from)?;
        let request = ClassManagerRequest::AddClass(raw);
        match self.send(request).await? {
            ClassManagerResponse::AddClass(res) => res.map_err(Into::into),
            _ => unreachable!("Mismatched response variant for AddClass"),
        }
    }

    async fn add_deprecated_class(
        &self,
        class_id: ClassId,
        class: DeprecatedClass,
    ) -> ClassManagerClientResult<()> {
        let raw = RawExecutableClass::try_from(ContractClass::V0(class))
            .map_err(ClassManagerError::from)?;
        let request = ClassManagerRequest::AddDeprecatedClass(class_id, raw);
        match self.send(request).await? {
            ClassManagerResponse::AddDeprecatedClass(res) => res.map_err(Into::into),
            _ => unreachable!("Mismatched response variant for AddDeprecatedClass"),
        }
    }

    async fn get_casm_v1(
        &self,
        class_id: ClassId,
    ) -> ClassManagerClientResult<Option<RawCasmContractClass>> {
        let request = ClassManagerRequest::GetCasmV1(class_id);
        match self.send(request).await? {
            ClassManagerResponse::GetCasmV1(res) => res.map_err(ClassManagerClientError::from),
            _ => unreachable!("Mismatched response variant for GetCasmV1"),
        }
    }

    async fn get_deprecated_executable(
        &self,
        class_id: ClassId,
    ) -> ClassManagerClientResult<Option<RawDeprecatedExecutableClass>> {
        let request = ClassManagerRequest::GetDeprecatedExecutable(class_id);
        match self.send(request).await? {
            ClassManagerResponse::GetDeprecatedExecutable(res) => {
                res.map_err(ClassManagerClientError::from)
            }
            _ => unreachable!("Mismatched response variant for GetDeprecatedExecutable"),
        }
    }

    async fn get_executable(
        &self,
        class_id: ClassId,
    ) -> ClassManagerClientResult<Option<ExecutableClass>> {
        // Prefer V1
        if let Some(raw_casm) = self.get_casm_v1(class_id).await? {
            // Reconstruct Sierra version from Sierra class
            let Some(sierra) = self.get_sierra(class_id).await? else { return Ok(None) };
            let sierra_version = starknet_api::contract_class::SierraVersion::extract_from_program(
                &sierra.sierra_program,
            )
            .map_err(|e| ClassManagerError::Client(e.to_string()))?;
            let casm = apollo_compile_to_casm_types::RawCasmContractClass::try_into(raw_casm)
                .map_err(ClassManagerError::from)?;
            return Ok(Some(ExecutableClass::V1((casm, sierra_version))));
        }

        // Fallback to V0
        let deprecated = self.get_deprecated_executable(class_id).await?;
        let deprecated = deprecated
            .map(starknet_api::deprecated_contract_class::ContractClass::try_from)
            .transpose()
            .map_err(ClassManagerError::from)?;
        Ok(deprecated.map(ExecutableClass::V0))
    }

    async fn get_sierra(&self, class_id: ClassId) -> ClassManagerClientResult<Option<Class>> {
        let request = ClassManagerRequest::GetSierra(class_id);
        let raw_opt = match self.send(request).await? {
            ClassManagerResponse::GetSierra(res) => res.map_err(ClassManagerClientError::from)?,
            _ => unreachable!("Mismatched response variant for GetSierra"),
        };
        let converted =
            raw_opt.map(Class::try_from).transpose().map_err(ClassManagerError::from)?;
        Ok(converted)
    }

    async fn add_class_and_executable_unsafe(
        &self,
        class_id: ClassId,
        class: Class,
        executable_class_id: ExecutableClassHash,
        executable_class: ExecutableClass,
    ) -> ClassManagerClientResult<()> {
        let raw_class = RawClass::try_from(class).map_err(ClassManagerError::from)?;
        let raw_executable =
            RawExecutableClass::try_from(executable_class).map_err(ClassManagerError::from)?;
        let request = ClassManagerRequest::AddClassAndExecutableUnsafe(
            class_id,
            raw_class,
            executable_class_id,
            raw_executable,
        );
        match self.send(request).await? {
            ClassManagerResponse::AddClassAndExecutableUnsafe(res) => res.map_err(Into::into),
            _ => unreachable!("Mismatched response variant for AddClassAndExecutableUnsafe"),
        }
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

    async fn get_casm_v1(
        &self,
        _class_id: ClassId,
    ) -> ClassManagerClientResult<Option<RawCasmContractClass>> {
        Ok(None)
    }

    async fn get_deprecated_executable(
        &self,
        _class_id: ClassId,
    ) -> ClassManagerClientResult<Option<RawDeprecatedExecutableClass>> {
        Ok(None)
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
