use serde::Deserialize;
use starknet_api::define_versioned_constants;

/// Versioned constants for the Consensus.
#[derive(Clone, Deserialize)]
pub struct VersionedConstants {
    ///  This is used to calculate the base gas price for the next block according to EIP-1559 and
    /// serves as a sensitivity parameter that limits the maximum rate of change of the gas price
    /// between consecutive blocks.
    pub gas_price_max_change_denominator: u128,
    /// The minimum gas price in fri.
    pub min_gas_price: u64,
    /// The maximum block size in gas units.
    pub max_block_size: u64,
    /// The target gas usage per block (usually half of a block's gas limit).
    pub gas_target: u64,
}

// TODO(Ayelet): Change version and path to 0.14 once added.
define_versioned_constants!(
    VersionedConstants,
    (V0_13_4, "../resources/versioned_constants_0_13_4.json"),
);
