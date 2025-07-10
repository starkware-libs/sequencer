use std::collections::BTreeMap;
use std::fmt::Debug;
use std::time::Duration;

use apollo_config::converters::deserialize_milliseconds_to_duration;
use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::core::{ChainId, ContractAddress};
use validator::Validate;

const GWEI_FACTOR: u128 = u128::pow(10, 9);
const ETH_FACTOR: u128 = u128::pow(10, 18);

/// Configuration for the Context struct.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Validate)]
pub struct ContextConfig {
    /// Buffer size for streaming outbound proposals.
    pub proposal_buffer_size: usize,
    /// The number of validators.
    pub num_validators: u64,
    /// The chain id of the Starknet chain.
    pub chain_id: ChainId,
    /// Maximum allowed deviation (seconds) of a proposed block's timestamp from the current time.
    pub block_timestamp_window_seconds: u64,
    /// The data availability mode, true: Blob, false: Calldata.
    pub l1_da_mode: bool,
    /// The address of the contract that builds the block.
    pub builder_address: ContractAddress,
    /// Safety margin in milliseconds to make sure that the batcher completes building the proposal
    /// with enough time for the Fin to be checked by validators.
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub build_proposal_margin_millis: Duration,
    // When validating a proposal the Context is responsible for timeout handling. The Batcher
    // though has a timeout as a defensive measure to make sure the proposal doesn't live
    // forever if the Context crashes or has a bug.
    /// Safety margin in milliseconds to allow the batcher to successfully validate a proposal.
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub validate_proposal_margin_millis: Duration,
    /// The minimum L1 gas price in wei.
    pub min_l1_gas_price_wei: u128,
    /// The maximum L1 gas price in wei.
    pub max_l1_gas_price_wei: u128,
    /// The minimum L1 data gas price in wei.
    pub min_l1_data_gas_price_wei: u128,
    /// The maximum L1 data gas price in wei.
    pub max_l1_data_gas_price_wei: u128,
    /// Part per thousand of multiplicative factor to apply to the data gas price, to enable
    /// fine-tuning of the price charged to end users. Commonly used to apply a discount due to
    /// the blob's data being compressed. Can be used to raise the prices in case of blob
    /// under-utilization.
    pub l1_data_gas_price_multiplier_ppt: u128,
    /// This additional gas is added to the L1 gas price.
    pub l1_gas_tip_wei: u128,
    /// If true, sets STRK gas price to its minimum price in versioned constants.
    pub constant_l2_gas_price: bool,
}

impl SerializeConfig for ContextConfig {
    fn dump(&self) -> BTreeMap<ParamPath, SerializedParam> {
        BTreeMap::from_iter([
            ser_param(
                "proposal_buffer_size",
                &self.proposal_buffer_size,
                "The buffer size for streaming outbound proposals.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "num_validators",
                &self.num_validators,
                "The number of validators.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "chain_id",
                &self.chain_id,
                "The chain id of the Starknet chain.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "block_timestamp_window_seconds",
                &self.block_timestamp_window_seconds,
                "Maximum allowed deviation (seconds) of a proposed block's timestamp from the \
                 current time.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "l1_da_mode",
                &self.l1_da_mode,
                "The data availability mode, true: Blob, false: Calldata.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "builder_address",
                &self.builder_address,
                "The address of the contract that builds the block.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "build_proposal_margin_millis",
                &self.build_proposal_margin_millis.as_millis(),
                "Safety margin (in ms) to make sure that the batcher completes building the \
                 proposal with enough time for the Fin to be checked by validators.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "validate_proposal_margin_millis",
                &self.validate_proposal_margin_millis.as_millis(),
                "Safety margin (in ms) to make sure that consensus determines when to timeout \
                 validating a proposal.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "min_l1_gas_price_wei",
                &self.min_l1_gas_price_wei,
                "The minimum L1 gas price in wei.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_l1_gas_price_wei",
                &self.max_l1_gas_price_wei,
                "The maximum L1 gas price in wei.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "min_l1_data_gas_price_wei",
                &self.min_l1_data_gas_price_wei,
                "The minimum L1 data gas price in wei.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "max_l1_data_gas_price_wei",
                &self.max_l1_data_gas_price_wei,
                "The maximum L1 data gas price in wei.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "l1_data_gas_price_multiplier_ppt",
                &self.l1_data_gas_price_multiplier_ppt,
                "Part per thousand of multiplicative factor to apply to the data gas price, to \
                 enable fine-tuning of the price charged to end users.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "l1_gas_tip_wei",
                &self.l1_gas_tip_wei,
                "This additional gas is added to the L1 gas price.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "constant_l2_gas_price",
                &self.constant_l2_gas_price,
                "If true, sets STRK gas price to its minimum price in versioned constants.",
                ParamPrivacyInput::Public,
            ),
        ])
    }
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            proposal_buffer_size: 100,
            num_validators: 1,
            chain_id: ChainId::Mainnet,
            block_timestamp_window_seconds: 1,
            l1_da_mode: true,
            builder_address: ContractAddress::default(),
            build_proposal_margin_millis: Duration::from_millis(1000),
            validate_proposal_margin_millis: Duration::from_millis(10_000),
            min_l1_gas_price_wei: GWEI_FACTOR,
            max_l1_gas_price_wei: 200 * GWEI_FACTOR,
            min_l1_data_gas_price_wei: 1,
            max_l1_data_gas_price_wei: ETH_FACTOR,
            l1_data_gas_price_multiplier_ppt: 135,
            l1_gas_tip_wei: GWEI_FACTOR,
            constant_l2_gas_price: false,
        }
    }
}
