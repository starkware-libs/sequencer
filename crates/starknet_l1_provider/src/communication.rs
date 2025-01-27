use async_trait::async_trait;
use starknet_l1_provider_types::{L1ProviderRequest, L1ProviderResponse};
use starknet_sequencer_infra::component_client::{LocalComponentClient, RemoteComponentClient};
use starknet_sequencer_infra::component_definitions::{
    ComponentRequestAndResponseSender,
    ComponentRequestHandler,
};
use starknet_sequencer_infra::component_server::{
    LocalComponentServer,
    RemoteComponentServer,
    WrapperServer,
};
use tracing::instrument;

pub type LocalL1ProviderServer =
    LocalComponentServer<L1Provider, L1ProviderRequest, L1ProviderResponse>;
pub type RemoteL1ProviderServer = RemoteComponentServer<L1ProviderRequest, L1ProviderResponse>;
pub type L1ProviderRequestAndResponseSender =
    ComponentRequestAndResponseSender<L1ProviderRequest, L1ProviderResponse>;
pub type LocalL1ProviderClient = LocalComponentClient<L1ProviderRequest, L1ProviderResponse>;
pub type RemoteL1ProviderClient = RemoteComponentClient<L1ProviderRequest, L1ProviderResponse>;

use crate::l1_provider::L1Provider;
use crate::l1_scraper::L1Scraper;

pub type L1ScraperServer<B> = WrapperServer<L1Scraper<B>>;

#[async_trait]
impl ComponentRequestHandler<L1ProviderRequest, L1ProviderResponse> for L1Provider {
    #[instrument(skip(self))]
    async fn handle_request(&mut self, request: L1ProviderRequest) -> L1ProviderResponse {
        match request {
            L1ProviderRequest::AddEvents(events) => {
                L1ProviderResponse::AddEvents(self.process_l1_events(events))
            }
            L1ProviderRequest::CommitBlock { l1_handler_tx_hashes, height } => {
                L1ProviderResponse::CommitBlock(self.commit_block(&l1_handler_tx_hashes, height))
            }
            L1ProviderRequest::GetTransactions { n_txs, height } => {
                L1ProviderResponse::GetTransactions(self.get_txs(n_txs, height))
            }
            L1ProviderRequest::StartBlock { state, height } => {
                L1ProviderResponse::StartBlock(self.start_block(height, state))
            }
            L1ProviderRequest::Validate { tx_hash, height } => {
                L1ProviderResponse::Validate(self.validate(tx_hash, height))
            }
            L1ProviderRequest::Initialize(events) => {
                L1ProviderResponse::Initialize(self.initialize(events).await)
            }
        }
    }
}
