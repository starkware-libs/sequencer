use serde::Deserialize;
use starknet_api::block::{GasPrice, StarknetVersion};
use starknet_api::define_versioned_constants;
use starknet_api::execution_resources::GasAmount;
use thiserror::Error;

/// Versioned constants for the Consensus.
#[derive(Clone, Deserialize)]
pub struct VersionedConstants {
    ///  This is used to calculate the base gas price for the next block according to EIP-1559 and
    /// serves as a sensitivity parameter that limits the maximum rate of change of the gas price
    /// between consecutive blocks.
    pub gas_price_max_change_denominator: u128,
    /// The minimum gas price in fri.
    pub min_gas_price: GasPrice,
    /// The maximum block size in gas units.
    pub max_block_size: GasAmount,
    /// The target gas usage per block (usually half of a block's gas limit).
    pub gas_target: GasAmount,
    /// The margin for the eth to fri rate disagreement, expressed as a percentage (parts per
    /// hundred).
    pub l1_gas_price_margin_percent: u32,
}

define_versioned_constants!(
    VersionedConstants,
    VersionedConstantsError,
    (V0_14_0, "../resources/orchestrator_versioned_constants_0_14_0.json"),
    (V0_15_0, "../resources/orchestrator_versioned_constants_0_15_0.json"),
);

/// Error type for the Consensus' versioned constants.
#[derive(Debug, Error)]
pub enum VersionedConstantsError {
    /// Invalid Starknet version.
    #[error("Invalid Starknet version: {0}")]
    InvalidStarknetVersion(StarknetVersion),
}
