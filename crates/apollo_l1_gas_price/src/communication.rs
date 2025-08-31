use apollo_infra::component_client::{LocalComponentClient, RemoteComponentClient};
use apollo_infra::component_definitions::{ComponentRequestHandler, RequestWrapper};
use apollo_infra::component_server::{LocalComponentServer, RemoteComponentServer, WrapperServer};
use apollo_l1_gas_price_types::{L1GasPriceRequest, L1GasPriceResponse};
use async_trait::async_trait;
use papyrus_base_layer::monitored_base_layer::MonitoredBaseLayer;
use tracing::instrument;

use crate::l1_gas_price_provider::L1GasPriceProvider;
use crate::l1_gas_price_scraper::L1GasPriceScraper;

pub type LocalL1GasPriceServer =
    LocalComponentServer<L1GasPriceProvider, L1GasPriceRequest, L1GasPriceResponse>;
pub type RemoteL1GasPriceServer = RemoteComponentServer<L1GasPriceRequest, L1GasPriceResponse>;
pub type L1GasPriceRequestWrapper = RequestWrapper<L1GasPriceRequest, L1GasPriceResponse>;
pub type LocalL1GasPriceClient = LocalComponentClient<L1GasPriceRequest, L1GasPriceResponse>;
pub type RemoteL1GasPriceClient = RemoteComponentClient<L1GasPriceRequest, L1GasPriceResponse>;

pub type L1GasPriceScraperServer<B> = WrapperServer<L1GasPriceScraper<MonitoredBaseLayer<B>>>;

#[async_trait]
impl ComponentRequestHandler<L1GasPriceRequest, L1GasPriceResponse> for L1GasPriceProvider {
    #[instrument(skip(self))]
    async fn handle_request(&mut self, request: L1GasPriceRequest) -> L1GasPriceResponse {
        match request {
            L1GasPriceRequest::Initialize => L1GasPriceResponse::Initialize(self.initialize()),
            L1GasPriceRequest::GetGasPrice(timestamp) => {
                L1GasPriceResponse::GetGasPrice(self.get_price_info(timestamp))
            }
            L1GasPriceRequest::AddGasPrice(data) => {
                L1GasPriceResponse::AddGasPrice(self.add_price_info(data))
            }
            L1GasPriceRequest::GetEthToFriRate(timestamp) => {
                L1GasPriceResponse::GetEthToFriRate(self.eth_to_fri_rate(timestamp).await)
            }
        }
    }
}
