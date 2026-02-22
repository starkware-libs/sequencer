use apollo_infra::component_client::{LocalComponentClient, RemoteComponentClient};
use apollo_infra::component_definitions::{ComponentRequestHandler, RequestWrapper};
use apollo_infra::component_server::{LocalComponentServer, RemoteComponentServer, WrapperServer};
use apollo_l1_events_types::{L1EventsProviderRequest, L1EventsProviderResponse};
use async_trait::async_trait;
use tracing::instrument;

pub type LocalL1EventsProviderServer =
    LocalComponentServer<L1EventsProvider, L1EventsProviderRequest, L1EventsProviderResponse>;
pub type RemoteL1EventsProviderServer =
    RemoteComponentServer<L1EventsProviderRequest, L1EventsProviderResponse>;
pub type L1EventsProviderRequestWrapper =
    RequestWrapper<L1EventsProviderRequest, L1EventsProviderResponse>;
pub type LocalL1EventsProviderClient =
    LocalComponentClient<L1EventsProviderRequest, L1EventsProviderResponse>;
pub type RemoteL1EventsProviderClient =
    RemoteComponentClient<L1EventsProviderRequest, L1EventsProviderResponse>;

use crate::l1_events_provider::L1EventsProvider;
use crate::l1_scraper::L1EventsScraper;

pub type L1EventsScraperServer<B> = WrapperServer<L1EventsScraper<B>>;

#[async_trait]
impl ComponentRequestHandler<L1EventsProviderRequest, L1EventsProviderResponse>
    for L1EventsProvider
{
    #[instrument(skip(self))]
    async fn handle_request(
        &mut self,
        request: L1EventsProviderRequest,
    ) -> L1EventsProviderResponse {
        match request {
            L1EventsProviderRequest::AddEvents(events) => {
                L1EventsProviderResponse::AddEvents(self.add_events(events))
            }
            L1EventsProviderRequest::CommitBlock {
                l1_handler_tx_hashes,
                rejected_tx_hashes,
                height,
            } => L1EventsProviderResponse::CommitBlock(self.commit_block(
                l1_handler_tx_hashes,
                rejected_tx_hashes,
                height,
            )),
            L1EventsProviderRequest::GetTransactions { n_txs, height } => {
                L1EventsProviderResponse::GetTransactions(self.get_txs(n_txs, height))
            }
            L1EventsProviderRequest::StartBlock { state, height } => {
                L1EventsProviderResponse::StartBlock(self.start_block(height, state))
            }
            L1EventsProviderRequest::Validate { tx_hash, height } => {
                L1EventsProviderResponse::Validate(self.validate(tx_hash, height))
            }
            L1EventsProviderRequest::Initialize { historic_l2_height, events } => {
                L1EventsProviderResponse::Initialize(
                    self.initialize(historic_l2_height, events).await,
                )
            }
            L1EventsProviderRequest::GetL1EventsProviderSnapshot => {
                L1EventsProviderResponse::GetL1EventsProviderSnapshot(
                    self.get_l1_events_provider_snapshot(),
                )
            }
            L1EventsProviderRequest::GetProviderState => {
                L1EventsProviderResponse::GetProviderState(self.get_provider_state())
            }
        }
    }
}
