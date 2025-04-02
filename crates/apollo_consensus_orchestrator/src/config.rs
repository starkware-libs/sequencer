use std::collections::BTreeMap;
use std::fmt::Debug;
use std::time::Duration;

use apollo_config::converters::deserialize_milliseconds_to_duration;
use apollo_config::dumping::{ser_param, SerializeConfig};
use apollo_config::{ParamPath, ParamPrivacyInput, SerializedParam};
use serde::{Deserialize, Serialize};
use starknet_api::core::{ChainId, ContractAddress};
use validator::Validate;

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
    pub block_timestamp_window: u64,
    /// The data availability mode, true: Blob, false: Calldata.
    pub l1_da_mode: bool,
    /// The address of the contract that builds the block.
    pub builder_address: ContractAddress,
    /// Safety margin in milliseconds to make sure that the batcher completes building the proposal
    /// with enough time for the Fin to be checked by validators.
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub build_proposal_margin: Duration,
    // When validating a proposal the Context is responsible for timeout handling. The Batcher
    // though has a timeout as a defensive measure to make sure the proposal doesn't live
    // forever if the Context crashes or has a bug.
    /// Safety margin in milliseconds to allow the batcher to successfully validate a proposal.
    #[serde(deserialize_with = "deserialize_milliseconds_to_duration")]
    pub validate_proposal_margin: Duration,
    /// The minimum L1 gas price in wei.
    pub min_l1_gas_price_wei: u128,
    /// The maximum L1 gas price in wei.
    pub max_l1_gas_price_wei: u128,
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
                "block_timestamp_window",
                &self.block_timestamp_window,
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
                "build_proposal_margin",
                &self.build_proposal_margin.as_millis(),
                "Safety margin (in ms) to make sure that the batcher completes building the \
                 proposal with enough time for the Fin to be checked by validators.",
                ParamPrivacyInput::Public,
            ),
            ser_param(
                "validate_proposal_margin",
                &self.validate_proposal_margin.as_millis(),
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
        ])
    }
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            proposal_buffer_size: 100,
            num_validators: 1,
            chain_id: ChainId::Mainnet,
            block_timestamp_window: 1,
            l1_da_mode: true,
            builder_address: ContractAddress::default(),
            build_proposal_margin: Duration::from_millis(1000),
            validate_proposal_margin: Duration::from_millis(10_000),
            // TODO(guyn): what are the appropriate values for min/max l1 gas prices?
            min_l1_gas_price_wei: 1,
            max_l1_gas_price_wei: 1_000_000,
        }
    }
}
