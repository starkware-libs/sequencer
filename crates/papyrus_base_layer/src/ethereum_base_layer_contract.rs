use std::collections::BTreeMap;
use std::future::IntoFuture;
use std::ops::RangeInclusive;

use alloy::dyn_abi::SolType;
use alloy::eips::eip7840;
use alloy::primitives::Address as EthereumContractAddress;
use alloy::providers::network::Ethereum;
use alloy::providers::{Provider, ProviderBuilder, RootProvider};
use alloy::rpc::json_rpc::RpcError;
use alloy::rpc::types::eth::{
    BlockId,
    BlockNumberOrTag,
    BlockTransactionsKind,
    Filter as EthEventFilter,
};
use alloy::sol;
use alloy::sol_types::sol_data;
use alloy::transports::http::{Client, Http};
use alloy::transports::TransportErrorKind;
use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockNumber};
use starknet_api::hash::StarkHash;
use starknet_api::StarknetApiError;
use tracing::{debug, error};
use url::Url;
use validator::Validate;

use crate::{BaseLayerContract, L1BlockNumber, L1BlockReference, L1Event, PriceSample};

pub type EthereumBaseLayerResult<T> = Result<T, EthereumBaseLayerError>;

// Wraps the Starknet contract with a type that implements its interface, and is aware of its
// events.
sol!(
    #[sol(rpc)]
    Starknet,
    "resources/Starknet-0.10.3.4.json"
);

/// An interface that plays the role of the starknet L1 contract. It is able to create messages to
/// L2 from this contract, which appear on the corresponding base layer.
pub type StarknetL1Contract =
    Starknet::StarknetInstance<Http<Client>, RootProvider<Http<Client>>, Ethereum>;

#[derive(Clone, Debug)]
pub struct EthereumBaseLayerContract {
    pub config: EthereumBaseLayerConfig,
    pub contract: StarknetL1Contract,
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
    async fn get_proved_block_at(
        &self,
        l1_block: L1BlockNumber,
    ) -> EthereumBaseLayerResult<BlockHashAndNumber> {
        let block_id = l1_block.into();
        let call_state_block_number = self.contract.stateBlockNumber().block(block_id);
        let call_state_block_hash = self.contract.stateBlockHash().block(block_id);

        let (state_block_number, state_block_hash) = tokio::try_join!(
            call_state_block_number.call_raw().into_future(),
            call_state_block_hash.call_raw().into_future()
        )?;

        let validate = true;
        let block_number = sol_data::Uint::<64>::abi_decode(&state_block_number, validate)
            .inspect_err(|err| error!("{err}: {state_block_number}"))?;
        let block_hash = sol_data::FixedBytes::<32>::abi_decode(&state_block_hash, validate)
            .inspect_err(|err| error!("{err}: {state_block_hash}"))?;
        Ok(BlockHashAndNumber {
            number: BlockNumber(block_number),
            hash: BlockHash(StarkHash::from_bytes_be(&block_hash)),
        })
    }

    /// Returns the latest proved block on Ethereum, where finality determines how many
    /// blocks back (0 = latest).
    async fn latest_proved_block(
        &self,
        finality: u64,
    ) -> EthereumBaseLayerResult<Option<BlockHashAndNumber>> {
        let Some(ethereum_block_number) = self.latest_l1_block_number(finality).await? else {
            return Ok(None);
        };
        self.get_proved_block_at(ethereum_block_number).await.map(Some)
    }

    async fn events<'a>(
        &'a self,
        block_range: RangeInclusive<u64>,
        events: &'a [&'a str],
    ) -> EthereumBaseLayerResult<Vec<L1Event>> {
        let filter = EthEventFilter::new().select(block_range.clone()).events(events);

        let matching_logs = self.contract.provider().get_logs(&filter).await?;
        let received_tx_hashes: Vec<_> =
            matching_logs.iter().map(|log| log.transaction_hash).collect();
        debug!(
            "Got events of blocks in range {:?} with transaction hash: {:?}",
            block_range, received_tx_hashes
        );
        matching_logs.into_iter().map(L1Event::try_from).collect()
    }

    async fn latest_l1_block_number(
        &self,
        finality: u64,
    ) -> EthereumBaseLayerResult<Option<L1BlockNumber>> {
        Ok(self.contract.provider().get_block_number().await?.checked_sub(finality))
    }

    async fn latest_l1_block(
        &self,
        finality: u64,
    ) -> EthereumBaseLayerResult<Option<L1BlockReference>> {
        let Some(block_number) = self.latest_l1_block_number(finality).await? else {
            return Ok(None);
        };

        self.l1_block_at(block_number).await
    }

    async fn l1_block_at(
        &self,
        block_number: L1BlockNumber,
    ) -> EthereumBaseLayerResult<Option<L1BlockReference>> {
        let only_block_header: BlockTransactionsKind = BlockTransactionsKind::default();
        let block = self
            .contract
            .provider()
            .get_block(BlockId::Number(block_number.into()), only_block_header)
            .await?;

        Ok(block.map(|block| L1BlockReference {
            number: block.header.number,
            hash: block.header.hash.0,
        }))
    }

    // Query the Ethereum base layer for the timestamp, gas price, and data gas price of a block.
    async fn get_price_sample_and_block_reference(
        &self,
        block_number: L1BlockNumber,
    ) -> EthereumBaseLayerResult<Option<(PriceSample, L1BlockReference)>> {
        let block = self
            .contract
            .provider()
            .get_block(
                BlockId::Number(BlockNumberOrTag::Number(block_number)),
                BlockTransactionsKind::Hashes,
            )
            .await?;
        let Some(block) = block else {
            return Ok(None);
        };

        let Some(base_fee_per_gas) = block.header.base_fee_per_gas else {
            return Ok(None);
        };
        let blob_fee = match block.header.excess_blob_gas {
            Some(excess_blob_gas) if self.config.prague_blob_gas_calc => {
                // Pectra update.
                eip7840::BlobParams::prague().calc_blob_fee(excess_blob_gas)
            }
            Some(excess_blob_gas) => {
                // EIP 4844 - original blob pricing.
                eip7840::BlobParams::cancun().calc_blob_fee(excess_blob_gas)
            }
            None => 0,
        };
        let price_sample = PriceSample {
            timestamp: block.header.timestamp,
            base_fee_per_gas: base_fee_per_gas.into(),
            blob_fee,
        };
        let block_reference =
            L1BlockReference { number: block.header.number, hash: block.header.hash.0 };
        Ok(Some((price_sample, block_reference)))
    }
}

#[derive(thiserror::Error, Debug)]
pub enum EthereumBaseLayerError {
    #[error(transparent)]
    Contract(#[from] alloy::contract::Error),
    #[error("{0}")]
    FeeOutOfRange(alloy::primitives::ruint::FromUintError<u128>),
    #[error(transparent)]
    RpcError(#[from] RpcError<TransportErrorKind>),
    #[error("{0}")]
    StarknetApiParsingError(StarknetApiError),
    #[error(transparent)]
    TypeError(#[from] alloy::sol_types::Error),
    #[error("{0:?}")]
    UnhandledL1Event(alloy::primitives::Log),
}

impl PartialEq for EthereumBaseLayerError {
    fn eq(&self, other: &Self) -> bool {
        use EthereumBaseLayerError::*;
        match (self, other) {
            (Contract(this), Contract(other)) => this.to_string() == other.to_string(),
            (FeeOutOfRange(this), FeeOutOfRange(other)) => this == other,
            (RpcError(this), RpcError(other)) => this.to_string() == other.to_string(),
            (StarknetApiParsingError(this), StarknetApiParsingError(other)) => this == other,
            (TypeError(this), TypeError(other)) => this == other,
            (UnhandledL1Event(this), UnhandledL1Event(other)) => this == other,
            _ => false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Validate)]
pub struct EthereumBaseLayerConfig {
    pub node_url: Url,
    pub starknet_contract_address: EthereumContractAddress,
    pub prague_blob_gas_calc: bool,
}

impl SerializeConfig for EthereumBaseLayerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "node_url",
                &self.node_url.to_string(),
                "Ethereum node URL. A schema to match to Infura node: https://mainnet.infura.io/v3/<your_api_key>, but any other node can be used.",
                ParamPrivacyInput::Private,
            ),
            ser_param(
                "starknet_contract_address",
                &self.starknet_contract_address.to_string(),
                "Starknet contract address in ethereum.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "prague_blob_gas_calc",
                &self.prague_blob_gas_calc,
                "If true use the blob gas calculcation from the Pectra upgrade. If false use the EIP 4844 calculation.",
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
            prague_blob_gas_calc: true,
        }
    }
}
