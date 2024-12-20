use std::collections::BTreeMap;
use std::future::IntoFuture;

use alloy_dyn_abi::SolType;
use alloy_json_rpc::RpcError;
pub(crate) use alloy_primitives::Address as EthereumContractAddress;
use alloy_provider::network::Ethereum;
use alloy_provider::{Provider, ProviderBuilder, RootProvider};
use alloy_sol_types::{sol, sol_data};
use alloy_transport::TransportErrorKind;
use alloy_transport_http::{Client, Http};
use async_trait::async_trait;
use papyrus_config::dumping::{ser_param, ser_required_param, SerializeConfig};
use papyrus_config::{ParamPath, ParamPrivacyInput, SerializationType, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockNumber};
use starknet_api::hash::StarkHash;
use starknet_types_core::felt;
use url::Url;

use crate::{BaseLayerContract, L1Event};

type EthereumBaseLayerResult<T> = Result<T, EthereumBaseLayerError>;

// Wraps the Starknet contract with a type that implements its interface, and is aware of its
// events.
sol!(
    #[sol(rpc)]
    Starknet,
    "resources/Starknet-0.10.3.4.json"
);

#[derive(Debug)]
pub struct EthereumBaseLayerContract {
    pub config: EthereumBaseLayerConfig,
    pub contract: Starknet::StarknetInstance<Http<Client>, RootProvider<Http<Client>>, Ethereum>,
}

impl EthereumBaseLayerContract {
    pub fn new(config: EthereumBaseLayerConfig) -> Self {
        let l1_client = ProviderBuilder::new().on_http(config.node_url.clone());
        // This type is generated from `sol!` macro, and the `new` method assumes it is already
        // deployed at L1, and wraps it with a type.
        let contract = Starknet::new(config.starknet_contract_address, l1_client);
        Self { contract, config }
    }
}

#[async_trait]
impl BaseLayerContract for EthereumBaseLayerContract {
    type Error = EthereumBaseLayerError;

    /// Returns the latest proved block on Ethereum, where finality determines how many
    /// blocks back (0 = latest).
    async fn latest_proved_block(
        &self,
        finality: u64,
    ) -> EthereumBaseLayerResult<Option<BlockHashAndNumber>> {
        let Some(ethereum_block_number) = self.latest_l1_block_number(finality).await? else {
            return Ok(None);
        };

        let call_state_block_number =
            self.contract.stateBlockNumber().block(ethereum_block_number.into());
        let call_state_block_hash =
            self.contract.stateBlockHash().block(ethereum_block_number.into());

        let (state_block_number, state_block_hash) = tokio::try_join!(
            call_state_block_number.call_raw().into_future(),
            call_state_block_hash.call_raw().into_future()
        )?;

        let validate = true;
        let block_number = sol_data::Uint::<64>::abi_decode(&state_block_number, validate)?;
        let block_hash = sol_data::FixedBytes::<32>::abi_decode(&state_block_hash, validate)?;
        Ok(Some(BlockHashAndNumber {
            number: BlockNumber(block_number),
            hash: BlockHash(StarkHash::from_bytes_be(&block_hash)),
        }))
    }

    async fn events(
        &self,
        _from_block: u64,
        _until_block: u64,
        _event_identifiers: &[&str],
    ) -> EthereumBaseLayerResult<Vec<L1Event>> {
        todo!("Implmeneted in a subsequent commit")
    }

    async fn latest_l1_block_number(&self, finality: u64) -> EthereumBaseLayerResult<Option<u64>> {
        Ok(self.contract.provider().get_block_number().await?.checked_sub(finality))
    }
}

#[derive(thiserror::Error, Debug)]
pub enum EthereumBaseLayerError {
    #[error(transparent)]
    Contract(#[from] alloy_contract::Error),
    #[error(transparent)]
    FeltParseError(#[from] felt::FromStrError),
    #[error(transparent)]
    RpcError(#[from] RpcError<TransportErrorKind>),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error(transparent)]
    TypeError(#[from] alloy_sol_types::Error),
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct EthereumBaseLayerConfig {
    pub node_url: Url,
    pub starknet_contract_address: EthereumContractAddress,
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
                &self.starknet_contract_address.to_string(),
                "Starknet contract address in ethereum.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Default for EthereumBaseLayerConfig {
    fn default() -> Self {
        let starknet_contract_address =
            "0xc662c410C0ECf747543f5bA90660f6ABeBD9C8c4".parse().unwrap();

        Self {
            node_url: "https://mainnet.infura.io/v3/<your_api_key>".parse().unwrap(),
            starknet_contract_address,
        }
    }
}
