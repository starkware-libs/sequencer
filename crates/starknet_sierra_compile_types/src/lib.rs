use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use papyrus_proc_macros::handle_response_variants;
use serde::{Deserialize, Serialize};
use starknet_api::core::CompiledClassHash;
use starknet_sequencer_infra::component_client::ClientError;
use starknet_sequencer_infra::component_definitions::ComponentClient;
use thiserror::Error;

pub type SierraCompilerResult<T> = Result<T, SierraCompilerError>;
pub type SierraCompilerClientResult<T> = Result<T, SierraCompilerClientError>;

pub type RawClass = Bytes;
pub type RawExecutableClass = (RawClass, CompiledClassHash);

pub type SharedSierraCompilerClient = Arc<dyn SierraCompilerClient>;

/// Serves as the Sierra compilation unit's shared interface.
/// Requires `Send + Sync` to allow transferring and sharing resources (inputs, futures) across
/// threads.
#[async_trait]
pub trait SierraCompilerClient: Send + Sync {
    async fn compile(&self, class: RawClass) -> SierraCompilerClientResult<RawExecutableClass>;
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
    Compile(SierraCompilerResult<RawExecutableClass>),
}

#[async_trait]
impl<ComponentClientType> SierraCompilerClient for ComponentClientType
where
    ComponentClientType:
        Send + Sync + ComponentClient<SierraCompilerRequest, SierraCompilerResponse>,
{
    async fn compile(&self, class: RawClass) -> SierraCompilerClientResult<RawExecutableClass> {
        let request = SierraCompilerRequest::Compile(class);
        let response = self.send(request).await;
        handle_response_variants!(
            SierraCompilerResponse,
            Compile,
            SierraCompilerClientError,
            SierraCompilerError
        )
    }
}
