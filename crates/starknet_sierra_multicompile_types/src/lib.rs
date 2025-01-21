use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use papyrus_proc_macros::handle_all_response_variants;
use serde::{Deserialize, Serialize};
use starknet_api::contract_class::ContractClass;
use starknet_api::core::CompiledClassHash;
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
use thiserror::Error;

pub type SierraCompilerResult<T> = Result<T, SierraCompilerError>;
pub type SierraCompilerClientResult<T> = Result<T, SierraCompilerClientError>;

pub type RawExecutableHashedClass = (RawExecutableClass, CompiledClassHash);

pub type LocalSierraCompilerClient =
    LocalComponentClient<SierraCompilerRequest, SierraCompilerResponse>;
pub type RemoteSierraCompilerClient =
    RemoteComponentClient<SierraCompilerRequest, SierraCompilerResponse>;
pub type SharedSierraCompilerClient = Arc<dyn SierraCompilerClient>;
pub type SierraCompilerRequestAndResponseSender =
    ComponentRequestAndResponseSender<SierraCompilerRequest, SierraCompilerResponse>;

// TODO(Elin): change to a more efficient serde (bytes, or something similar).
// A prerequisite for this is to solve serde-untagged lack of support.

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RawClass(Bytes);

impl TryFrom<SierraContractClass> for RawClass {
    type Error = serde_json::Error;

    fn try_from(class: SierraContractClass) -> Result<Self, Self::Error> {
        let class = serde_json::to_vec(&class)?.into();
        Ok(Self(class))
    }
}

impl TryFrom<RawClass> for SierraContractClass {
    type Error = serde_json::Error;

    fn try_from(class: RawClass) -> Result<Self, Self::Error> {
        let class: SierraContractClass = serde_json::from_slice(class.0.as_ref())?;
        Ok(class)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RawExecutableClass(Bytes);

impl TryFrom<ContractClass> for RawExecutableClass {
    type Error = serde_json::Error;

    fn try_from(class: ContractClass) -> Result<Self, Self::Error> {
        let class = serde_json::to_vec(&class)?.into();
        Ok(Self(class))
    }
}

impl TryFrom<RawExecutableClass> for ContractClass {
    type Error = serde_json::Error;

    fn try_from(class: RawExecutableClass) -> Result<Self, Self::Error> {
        let class: ContractClass = serde_json::from_slice(class.0.as_ref())?;
        Ok(class)
    }
}

/// Serves as the Sierra compilation unit's shared interface.
/// Requires `Send + Sync` to allow transferring and sharing resources (inputs, futures) across
/// threads.
#[async_trait]
pub trait SierraCompilerClient: Send + Sync {
    async fn compile(
        &self,
        class: RawClass,
    ) -> SierraCompilerClientResult<RawExecutableHashedClass>;
}

#[derive(Clone, Debug, Error, Eq, PartialEq, Serialize, Deserialize)]
pub enum SierraCompilerError {
    #[error("Compilation failed: {0}")]
    SierraCompilerError(String),
}

#[derive(Clone, Debug, Error)]
pub enum SierraCompilerClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    SierraCompilerError(#[from] SierraCompilerError),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SierraCompilerRequest {
    Compile(RawClass),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SierraCompilerResponse {
    Compile(SierraCompilerResult<RawExecutableHashedClass>),
}

#[async_trait]
impl<ComponentClientType> SierraCompilerClient for ComponentClientType
where
    ComponentClientType:
        Send + Sync + ComponentClient<SierraCompilerRequest, SierraCompilerResponse>,
{
    async fn compile(
        &self,
        class: RawClass,
    ) -> SierraCompilerClientResult<RawExecutableHashedClass> {
        let request = SierraCompilerRequest::Compile(class);
        handle_all_response_variants!(
            SierraCompilerResponse,
            Compile,
            SierraCompilerClientError,
            SierraCompilerError,
            Direct
        )
    }
}
