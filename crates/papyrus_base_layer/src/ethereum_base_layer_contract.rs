use std::collections::BTreeMap;
use std::future::IntoFuture;

use alloy_contract::{ContractInstance, Interface};
use alloy_dyn_abi::SolType;
use alloy_primitives::Address;
use alloy_provider::network::Ethereum;
use alloy_provider::{Provider, ProviderBuilder, RootProvider};
use alloy_sol_types::sol_data;
use alloy_transport::TransportErrorKind;
use alloy_transport_http::{Client, Http};
use async_trait::async_trait;
use papyrus_config::dumping::{ser_param, ser_required_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializationType, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockNumber};
use starknet_api::hash::StarkHash;
use url::ParseError;

use crate::BaseLayerContract;

#[derive(thiserror::Error, Debug)]
pub enum EthereumBaseLayerError {
    #[error(transparent)]
    FromHex(#[from] alloy_primitives::hex::FromHexError),
    #[error(transparent)]
    Url(#[from] ParseError),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    Contract(#[from] alloy_contract::Error),
    #[error(transparent)]
    TypeError(#[from] alloy_sol_types::Error),
    #[error(transparent)]
    RpcError(#[from] alloy_json_rpc::RpcError<TransportErrorKind>),
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct EthereumBaseLayerConfig {
    // TODO(yair): consider using types.
    pub node_url: String,
    pub starknet_contract_address: String,
}

impl SerializeConfig for EthereumBaseLayerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_required_param(
                "node_url",
                SerializationType::String,
                "Ethereum node URL. A schema to match to Infura node: https://mainnet.infura.io/v3/<your_api_key>, but any other node can be used.",
                ParamPrivacyInput::Private,
            ),
            ser_param(
                "starknet_contract_address",
                &self.starknet_contract_address,
                "Starknet contract address in ethereum.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Default for EthereumBaseLayerConfig {
    fn default() -> Self {
        Self {
            node_url: "https://mainnet.infura.io/v3/<your_api_key>".to_string(),
            starknet_contract_address: "0xc662c410C0ECf747543f5bA90660f6ABeBD9C8c4".to_string(),
        }
    }
}

#[derive(Debug)]
pub struct EthereumBaseLayerContract {
    contract: ContractInstance<Http<Client>, RootProvider<Http<Client>>, Ethereum>,
}

impl EthereumBaseLayerContract {
    pub fn new(config: EthereumBaseLayerConfig) -> Result<Self, EthereumBaseLayerError> {
        let address: Address = config.starknet_contract_address.parse()?;
        let client = ProviderBuilder::new().on_http(config.node_url.parse()?);

        // The solidity contract was pre-compiled, and only the relevant functions were kept.
        let abi = serde_json::from_str(include_str!("core_contract_latest_block.abi"))?;
        Ok(Self { contract: ContractInstance::new(address, client, Interface::new(abi)) })
    }
}

#[async_trait]
impl BaseLayerContract for EthereumBaseLayerContract {
    type Error = EthereumBaseLayerError;

    async fn latest_proved_block(
        &self,
        min_confirmations: Option<u64>,
    ) -> Result<Option<(BlockNumber, BlockHash)>, Self::Error> {
        let ethereum_block_number = self
            .contract
            .provider()
            .get_block_number()
            .await?
            .checked_sub(min_confirmations.unwrap_or_default());
        let Some(ethereum_block_number) = ethereum_block_number else {
            return Ok(None);
        };

        let call_state_block_number =
            self.contract.function("stateBlockNumber", &[])?.block(ethereum_block_number.into());
        let call_state_block_hash =
            self.contract.function("stateBlockHash", &[])?.block(ethereum_block_number.into());

        let (state_block_number, state_block_hash) = tokio::try_join!(
            call_state_block_number.call_raw().into_future(),
            call_state_block_hash.call_raw().into_future()
        )?;

        Ok(Some((
            BlockNumber(sol_data::Uint::<64>::abi_decode(&state_block_number, true)?),
            // TODO: use safe conversion.
            BlockHash(StarkHash::from_hex_unchecked(&state_block_hash.to_string())),
        )))
    }
}
