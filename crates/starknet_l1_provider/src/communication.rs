use async_trait::async_trait;
use starknet_l1_provider_types::{L1ProviderRequest, L1ProviderResponse};
use starknet_sequencer_infra::component_client::{LocalComponentClient, RemoteComponentClient};
use starknet_sequencer_infra::component_definitions::{
    ComponentRequestAndResponseSender,
    ComponentRequestHandler,
};
use starknet_sequencer_infra::component_server::{LocalComponentServer, RemoteComponentServer};
use tracing::instrument;

use crate::L1Provider;

pub type LocalL1ProviderServer =
    LocalComponentServer<L1Provider, L1ProviderRequest, L1ProviderResponse>;
pub type RemoteL1ProviderServer = RemoteComponentServer<L1ProviderRequest, L1ProviderResponse>;
pub type L1ProviderRequestAndResponseSender =
    ComponentRequestAndResponseSender<L1ProviderRequest, L1ProviderResponse>;
pub type LocalL1ProviderClient = LocalComponentClient<L1ProviderRequest, L1ProviderResponse>;
pub type RemoteL1ProviderClient = RemoteComponentClient<L1ProviderRequest, L1ProviderResponse>;

#[async_trait]
impl ComponentRequestHandler<L1ProviderRequest, L1ProviderResponse> for L1Provider {
    #[instrument(skip(self))]
    async fn handle_request(&mut self, request: L1ProviderRequest) -> L1ProviderResponse {
        match request {
            L1ProviderRequest::GetTransactions(n_txs) => {
                L1ProviderResponse::GetTransactions(self.get_txs(n_txs))
            }
        }
    }
}
