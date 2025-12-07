use std::collections::BTreeMap;
use std::future::IntoFuture;
use std::ops::RangeInclusive;
use std::time::Duration;

use alloy::dyn_abi::SolType;
use alloy::eips::eip7840;
use alloy::network::Ethereum;
use alloy::primitives::Address;
use alloy::providers::{Provider, ProviderBuilder, RootProvider};
use alloy::rpc::json_rpc::RpcError;
use alloy::rpc::types::eth::Filter as EthEventFilter;
use alloy::sol;
use alloy::sol_types::sol_data;
use alloy::transports::TransportErrorKind;
use apollo_config::converters::{
    deserialize_milliseconds_to_duration,
    deserialize_vec,
    serialize_slice,
};
use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::secrets::Sensitive;
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use starknet_api::block::{BlockHash, BlockHashAndNumber, BlockNumber};
use starknet_api::hash::StarkHash;
use starknet_api::StarknetApiError;
use tokio::time::error::Elapsed;
use tracing::{debug, error, instrument};
use url::Url;
use validator::{Validate, ValidationError};

use crate::eth_events::parse_event;
use crate::{
    BaseLayerContract,
    L1BlockHash,
    L1BlockHeader,
    L1BlockNumber,
    L1BlockReference,
    L1Event,
};

pub type EthereumBaseLayerResult<T> = Result<T, EthereumBaseLayerError>;
pub type EthereumContractAddress = Address;

#[cfg(test)]
#[path = "ethereum_base_layer_contract_test.rs"]
pub mod ethereum_base_layer_contract_test;

// Wraps the Starknet contract with a type that implements its interface, and is aware of its
// events.

#[cfg(any(test, feature = "testing"))]
// Mocked Starknet contract for testing (no governance).
sol!(
    #[sol(rpc)]
    Starknet,
    "resources/StarknetForSequencerTesting.json"
);
#[cfg(not(any(test, feature = "testing")))]
// Real Starknet contract for production.
sol!(
    #[sol(rpc)]
    Starknet,
    "resources/Starknet-0.10.3.4.json"
);

/// An interface that plays the role of the starknet L1 contract. It is able to create messages to
/// L2 from this contract, which appear on the corresponding base layer.
pub type StarknetL1Contract = Starknet::StarknetInstance<RootProvider, Ethereum>;

#[derive(Clone, Debug)]
pub struct CircularUrlIterator {
    urls: Vec<Sensitive<Url>>,
    index: usize,
}

impl CircularUrlIterator {
    pub fn new(urls: Vec<Sensitive<Url>>) -> Self {
        Self { urls, index: 0 }
    }

    pub fn get_current_url(&self) -> Sensitive<Url> {
        self.urls.get(self.index).cloned().expect("No endpoint URLs provided")
    }
}

impl Iterator for CircularUrlIterator {
    type Item = Sensitive<Url>;

    fn next(&mut self) -> Option<Self::Item> {
        self.index = (self.index + 1) % self.urls.len();
        self.urls.get(self.index).cloned()
    }
}

#[derive(Clone, Debug)]
pub struct EthereumBaseLayerContract {
    pub url_iterator: CircularUrlIterator,
    pub config: EthereumBaseLayerConfig,
    pub contract: StarknetL1Contract,
}

impl EthereumBaseLayerContract {
    pub fn new(config: EthereumBaseLayerConfig) -> Self {
        let url_iterator = CircularUrlIterator::new(config.ordered_l1_endpoint_urls.clone());
        let contract = build_contract_instance(
            config.starknet_contract_address,
            url_iterator.get_current_url(),
        );
        Self { url_iterator, contract, config }
    }
    #[cfg(any(test, feature = "testing"))]
    pub fn new_with_provider(config: EthereumBaseLayerConfig, provider: RootProvider) -> Self {
        let url_iterator = CircularUrlIterator::new(config.ordered_l1_endpoint_urls.clone());
        let starknet_contract_address = config.starknet_contract_address;
        let contract = Starknet::new(starknet_contract_address, provider);
        Self { url_iterator, contract, config }
    }
}

#[async_trait]
impl BaseLayerContract for EthereumBaseLayerContract {
    type Error = EthereumBaseLayerError;

    /// Get the Starknet block that is proved on the base layer at a specific L1 block number.
    #[instrument(skip(self), err)]
    async fn get_proved_block_at(
        &mut self,
        l1_block: L1BlockNumber,
    ) -> EthereumBaseLayerResult<BlockHashAndNumber> {
        let block_id = l1_block.into();
        let call_state_block_number = self.contract.stateBlockNumber().block(block_id);
        let call_state_block_hash = self.contract.stateBlockHash().block(block_id);

        let (state_block_number, state_block_hash) = tokio::try_join!(
            call_state_block_number.call_raw().into_future(),
            call_state_block_hash.call_raw().into_future()
        )?;

        let block_number = sol_data::Uint::<64>::abi_decode(&state_block_number)
            .inspect_err(|err| error!("{err}: {state_block_number}"))?;
        let block_hash = sol_data::FixedBytes::<32>::abi_decode(&state_block_hash)
            .inspect_err(|err| error!("{err}: {state_block_hash}"))?;
        Ok(BlockHashAndNumber {
            number: BlockNumber(block_number),
            hash: BlockHash(StarkHash::from_bytes_be(&block_hash)),
        })
    }

    #[instrument(skip(self), err)]
    async fn events<'a>(
        &'a mut self,
        block_range: RangeInclusive<u64>,
        event_types_to_filter: &'a [&'a str],
    ) -> EthereumBaseLayerResult<Vec<L1Event>> {
        // Don't actually need mutability here, and using mut self doesn't work with async move in
        // the loop below.
        let immutable_self = &*self;
        let filter = EthEventFilter::new()
            .select(block_range.clone())
            .events(event_types_to_filter)
            .address(immutable_self.config.starknet_contract_address);

        let matching_logs = tokio::time::timeout(
            immutable_self.config.timeout_millis,
            immutable_self.contract.provider().get_logs(&filter),
        )
        .await??;

        // Debugging.
        let hashes: Vec<_> = matching_logs.iter().filter_map(|log| log.transaction_hash).collect();
        debug!("Got events in {:?}, L1 tx hashes: {:?}", block_range, hashes);

        let block_header_futures = matching_logs.into_iter().map(|log| {
            let block_number = log.block_number.unwrap();
            async move {
                let header =
                    immutable_self.get_block_header_immutable(block_number).await?.unwrap();
                parse_event(log, header.timestamp)
            }
        });
        futures::future::join_all(block_header_futures).await.into_iter().collect()
    }

    #[instrument(skip(self), err)]
    async fn latest_l1_block_number(&mut self) -> EthereumBaseLayerResult<L1BlockNumber> {
        let block_number = tokio::time::timeout(
            self.config.timeout_millis,
            self.contract.provider().get_block_number(),
        )
        .await??;
        Ok(block_number)
    }

    #[instrument(skip(self), err)]
    async fn l1_block_at(
        &mut self,
        block_number: L1BlockNumber,
    ) -> EthereumBaseLayerResult<Option<L1BlockReference>> {
        let block = tokio::time::timeout(
            self.config.timeout_millis,
            self.contract.provider().get_block_by_number(block_number.into()),
        )
        .await??;

        Ok(block.map(|block| L1BlockReference {
            number: block.header.number,
            hash: L1BlockHash(block.header.hash.0),
        }))
    }

    /// Query the Ethereum base layer for the header of a block.
    #[instrument(skip(self), err)]
    async fn get_block_header(
        &mut self,
        block_number: L1BlockNumber,
    ) -> EthereumBaseLayerResult<Option<L1BlockHeader>> {
        self.get_block_header_immutable(block_number).await
    }

    async fn get_block_header_immutable(
        &self,
        block_number: L1BlockNumber,
    ) -> EthereumBaseLayerResult<Option<L1BlockHeader>> {
        let block = tokio::time::timeout(
            self.config.timeout_millis,
            self.contract.provider().get_block_by_number(block_number.into()),
        )
        .await??;
        let Some(block) = block else {
            return Ok(None);
        };
        let Some(base_fee) = block.header.base_fee_per_gas else {
            return Ok(None);
        };
        let blob_fee = match block.header.excess_blob_gas {
            Some(excess_blob_gas) if self.config.bpo2_start_block_number <= block.header.number => {
                // Fusaka BPO2 update.
                eip7840::BlobParams::bpo2().calc_blob_fee(excess_blob_gas)
            }
            Some(excess_blob_gas) if self.config.bpo1_start_block_number <= block.header.number => {
                // Fusaka BPO1 update.
                eip7840::BlobParams::bpo1().calc_blob_fee(excess_blob_gas)
            }
            Some(excess_blob_gas)
                if self.config.fusaka_no_bpo_start_block_number <= block.header.number =>
            {
                // Fusaka update.
                eip7840::BlobParams::osaka().calc_blob_fee(excess_blob_gas)
            }
            Some(excess_blob_gas) => {
                // Pectra update.
                eip7840::BlobParams::prague().calc_blob_fee(excess_blob_gas)
            }
            None => 0,
        };

        Ok(Some(L1BlockHeader {
            number: block.header.number,
            hash: L1BlockHash(block.header.hash.0),
            parent_hash: L1BlockHash(block.header.parent_hash.0),
            timestamp: block.header.timestamp.into(),
            base_fee_per_gas: base_fee.into(),
            blob_fee,
        }))
    }

    async fn get_url(&self) -> Result<Sensitive<Url>, Self::Error> {
        Ok(self.url_iterator.get_current_url())
    }

    /// Rebuilds the provider on the new url.
    async fn set_provider_url(&mut self, url: Sensitive<Url>) -> Result<(), Self::Error> {
        self.contract = build_contract_instance(self.config.starknet_contract_address, url.clone());
        Ok(())
    }

    async fn cycle_provider_url(&mut self) -> Result<(), Self::Error> {
        self.url_iterator
            .next()
            .expect("URL list was validated to be non-empty when config was loaded");
        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum EthereumBaseLayerError {
    #[error(transparent)]
    Contract(#[from] alloy::contract::Error),
    #[error("{0}")]
    FeeOutOfRange(alloy::primitives::ruint::FromUintError<u128>),
    #[error("timed-out while querying the L1 base layer")]
    ProviderTimeout(#[from] Elapsed),
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EthereumBaseLayerConfig {
    #[serde(deserialize_with = "deserialize_vec")]
    pub ordered_l1_endpoint_urls: Vec<Sensitive<Url>>,
    pub starknet_contract_address: EthereumContractAddress,
    // Note: dates of fusaka-related upgrades: https://eips.ethereum.org/EIPS/eip-7607
    // Note 2: make sure to calculate the block number as activation epoch x32.
    // The block number at which the Fusaka upgrade was deployed (not including any BPO updates).
    pub fusaka_no_bpo_start_block_number: L1BlockNumber,
    // The block number at which BPO1 update was deployed.
    pub bpo1_start_block_number: L1BlockNumber,
    // The block number at which BPO2 update was deployed.
    pub bpo2_start_block_number: L1BlockNumber,
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub timeout_millis: Duration,
}

impl Validate for EthereumBaseLayerConfig {
    fn validate(&self) -> Result<(), validator::ValidationErrors> {
        let mut errors = validator::ValidationErrors::new();

        // Check that the Fusaka updates are ordered chronologically.
        if self.fusaka_no_bpo_start_block_number > self.bpo1_start_block_number {
            let mut error = ValidationError::new("block_numbers_not_ordered");
            error.message =
                Some("fusaka_no_bpo_start_block_number must be <= bpo1_start_block_number".into());
            errors.add("fusaka_no_bpo_start_block_number", error);
        }

        if self.bpo1_start_block_number > self.bpo2_start_block_number {
            let mut error = ValidationError::new("block_numbers_not_ordered");
            error.message =
                Some("bpo1_start_block_number must be <= bpo2_start_block_number".into());
            errors.add("bpo1_start_block_number", error);
        }

        // Check that the URL list is not empty.
        if self.ordered_l1_endpoint_urls.is_empty() {
            let mut error = ValidationError::new("url_list_is_empty");
            error.message = Some("ordered_l1_endpoint_urls must not be empty".into());
            errors.add("ordered_l1_endpoint_urls", error);
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

impl SerializeConfig for EthereumBaseLayerConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "ordered_l1_endpoint_urls",
                &serialize_slice(
                    &self
                        .ordered_l1_endpoint_urls
                        .iter()
                        .map(|url| url.as_ref().clone())
                        .collect::<Vec<_>>(),
                ),
                "An ordered list of URLs for communicating with Ethereum. The list is used in \
                 order, cyclically, switching if the current one is non-operational.",
                ParamPrivacyInput::Private,
            ),
            ser_param(
                "starknet_contract_address",
                &self.starknet_contract_address.to_string(),
                "Starknet contract address in ethereum.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "fusaka_no_bpo_start_block_number",
                &self.fusaka_no_bpo_start_block_number,
                "The block number at which the Fusaka upgrade was deployed (not including any BPO \
                 updates).",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "bpo1_start_block_number",
                &self.bpo1_start_block_number,
                "The block number at which BPO1 update was deployed.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "bpo2_start_block_number",
                &self.bpo2_start_block_number,
                "The block number at which BPO2 update was deployed.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "timeout_millis",
                &self.timeout_millis.as_millis(),
                "The timeout (milliseconds) for a query of the L1 base layer",
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
            ordered_l1_endpoint_urls: vec![
                "https://mainnet.infura.io/v3/YOUR_INFURA_API_KEY".parse().unwrap(),
            ],
            starknet_contract_address,
            fusaka_no_bpo_start_block_number: 0,
            bpo1_start_block_number: 0,
            bpo2_start_block_number: 0,
            timeout_millis: Duration::from_millis(1000),
        }
    }
}

fn build_contract_instance(
    starknet_contract_address: EthereumContractAddress,
    node_url: Sensitive<Url>,
) -> StarknetL1Contract {
    let l1_client = ProviderBuilder::default().connect_http(node_url.expose_inner());
    // This type is generated from `sol!` macro, and the `new` method assumes it is already
    // deployed at L1, and wraps it with a type.
    Starknet::new(starknet_contract_address, l1_client)
}
