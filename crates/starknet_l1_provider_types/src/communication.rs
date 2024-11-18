use std::sync::Arc;

use async_trait::async_trait;
use mockall::predicate::*;
use mockall::*;
use papyrus_proc_macros::handle_response_variants;
use serde::{Deserialize, Serialize};
use starknet_sequencer_infra::component_client::ClientError;
use starknet_sequencer_infra::component_definitions::{
    ComponentClient,
    ComponentRequestAndResponseSender,
};
use thiserror::Error;
use tracing::instrument;

use crate::errors::L1ProviderError;
use crate::l1_provider_types::L1ProviderResult;

pub type L1ProviderClientResult<T> = Result<T, L1ProviderClientError>;
pub type L1ProviderRequestAndResponseSender =
    ComponentRequestAndResponseSender<L1ProviderRequest, L1ProviderResponse>;
pub type SharedL1ProviderClient = Arc<dyn L1ProviderClient>;

/// Serves as the l1-provider's shared interface. Requires `Send + Sync` to allow transferring and
/// sharing resources (inputs, futures) across threads.
#[automock]
#[async_trait]
pub trait L1ProviderClient: Send + Sync {
    /// Starts the process of updating internal L1 and L2 buffers.
    async fn start(&self) -> L1ProviderClientResult<()>;
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum L1ProviderRequest {
    Start(()),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum L1ProviderResponse {
    Start(L1ProviderResult<()>),
}

#[derive(Debug, Error)]
pub enum L1ProviderClientError {
    #[error(transparent)]
    ClientError(#[from] ClientError),
    #[error(transparent)]
    L1ProviderError(#[from] L1ProviderError),
}

#[async_trait]
impl<ComponentClientType> L1ProviderClient for ComponentClientType
where
    ComponentClientType: Send + Sync + ComponentClient<L1ProviderRequest, L1ProviderResponse>,
{
    #[instrument(skip(self))]
    async fn start(&self) -> L1ProviderClientResult<()> {
        let request = L1ProviderRequest::Start(());
        let response = self.send(request).await;
        handle_response_variants!(L1ProviderResponse, Start, L1ProviderClientError, L1ProviderError)
    }
}
