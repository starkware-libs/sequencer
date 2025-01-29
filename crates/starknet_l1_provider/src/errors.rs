use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerError;
use thiserror::Error;

// TODO(Gilad): move to scraper module once it's created.
#[derive(Error, Debug)]
pub enum L1ScraperError {
    #[error(transparent)]
    BaseLayer(#[from] EthereumBaseLayerError),
}
