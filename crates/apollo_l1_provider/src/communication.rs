use apollo_infra::component_client::{LocalComponentClient, RemoteComponentClient};
use apollo_infra::component_definitions::{ComponentRequestHandler, RequestWrapper};
use apollo_infra::component_server::{LocalComponentServer, RemoteComponentServer, WrapperServer};
use apollo_l1_provider_types::{L1ProviderRequest, L1ProviderResponse};
use async_trait::async_trait;
use papyrus_base_layer::monitored_base_layer::MonitoredBaseLayer;
use tracing::instrument;

pub type LocalL1ProviderServer =
    LocalComponentServer<L1Provider, L1ProviderRequest, L1ProviderResponse>;
pub type RemoteL1ProviderServer = RemoteComponentServer<L1ProviderRequest, L1ProviderResponse>;
pub type L1ProviderRequestWrapper = RequestWrapper<L1ProviderRequest, L1ProviderResponse>;
pub type LocalL1ProviderClient = LocalComponentClient<L1ProviderRequest, L1ProviderResponse>;
pub type RemoteL1ProviderClient = RemoteComponentClient<L1ProviderRequest, L1ProviderResponse>;

use crate::l1_provider::L1Provider;
use crate::l1_scraper::L1Scraper;

pub type L1ScraperServer<B> = WrapperServer<L1Scraper<MonitoredBaseLayer<B>>>;

#[async_trait]
impl ComponentRequestHandler<L1ProviderRequest, L1ProviderResponse> for L1Provider {
    #[instrument(skip(self))]
    async fn handle_request(&mut self, request: L1ProviderRequest) -> L1ProviderResponse {
        match request {
            L1ProviderRequest::AddEvents(events) => {
                L1ProviderResponse::AddEvents(self.add_events(events))
            }
            L1ProviderRequest::CommitBlock { l1_handler_tx_hashes, rejected_tx_hashes, height } => {
                L1ProviderResponse::CommitBlock(self.commit_block(
                    l1_handler_tx_hashes,
                    rejected_tx_hashes,
                    height,
                ))
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
            L1ProviderRequest::GetL1ProviderSnapshot => {
                L1ProviderResponse::GetL1ProviderSnapshot(self.get_l1_provider_snapshot())
            }
        }
    }
}
