use serde::Deserialize;
use starknet_api::block::GasPricePerToken;

#[derive(Debug, Deserialize)]
pub(crate) struct BlockInfo {
    pub da_mode: bool,
    pub l1_gas_price_per_token: GasPricePerToken,
    pub l1_data_gas_price_per_token: GasPricePerToken,
}
