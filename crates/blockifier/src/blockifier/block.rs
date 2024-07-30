use std::num::NonZeroU128;

use starknet_api::block::{BlockHash, BlockNumber, BlockTimestamp};
use starknet_api::core::ContractAddress;
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::abi::constants;
use crate::state::errors::StateError;
use crate::state::state_api::{State, StateResult};
use crate::transaction::objects::FeeType;
use crate::versioned_constants::{VersionedConstants, VersionedConstantsOverrides};

#[cfg(test)]
#[path = "block_test.rs"]
pub mod block_test;

#[derive(Clone, Debug)]
pub struct BlockInfo {
    pub block_number: BlockNumber,
    pub block_timestamp: BlockTimestamp,

    // Fee-related.
    pub sequencer_address: ContractAddress,
    pub gas_prices: GasPrices,
    pub use_kzg_da: bool,
}

impl BlockInfo {
    /// Calculate the base fee for the next block according to EIP-1559.
    ///
    /// # Parameters
    /// - `price`: The base fee of the current block.
    /// - `gas_used`: The total gas used in the current block.
    /// - `gas_target`: The target gas usage per block (usually half of the gas limit).
    pub fn calculate_next_base_gas_price(price: u64, gas_used: u64, gas_target: u64) -> u64 {
        // Sensitivity parameter that limits the maximum rate of change of the base fee between
        // consecutive blocks.
        // TODO(Mohammad): Ask product to provide the value of this constant.
        const GAS_PRICE_MAX_CHANGE_DENOMINATOR: u128 = 8;

        assert!(gas_target > 0, "Gas target must be positive");

        // We use unsigned integers (u64 and u128) to avoid overflow issues, as the input values are
        // naturally unsigned and i256 is unstable in Rust. This approach allows safe handling of
        // all inputs using u128 for intermediate calculations.

        // The difference between gas_used and gas_target is always u64.
        let gas_delta = gas_used.abs_diff(gas_target);
        // Convert to u128 to prevent overflow and because u64 * u64 results in u128.
        let price_u128 = u128::from(price);
        let gas_delta_u128 = u128::from(gas_delta);
        let gas_target_u128 = u128::from(gas_target);

        // Calculate the gas change as u128 to handle potential overflow during multiplication.
        let gas_delta_cost =
            price_u128.checked_mul(gas_delta_u128).expect("Both variables originate from u64");
        // Calculate the price change, maintaining precision by dividing after multiplication.
        // This avoids significant precision loss that would occur if dividing before
        // multiplication.
        let price_change_u128 = gas_delta_cost / gas_target_u128 / GAS_PRICE_MAX_CHANGE_DENOMINATOR;

        // Convert back to u64, as the price change should fit within the u64 range.
        let price_change = u64::try_from(price_change_u128).expect("Price change overflow");

        if gas_used > gas_target { price + price_change } else { price - price_change }
    }
}

#[derive(Clone, Debug)]
pub struct GasPrices {
    eth_l1_gas_price: NonZeroU128,       // In wei.
    strk_l1_gas_price: NonZeroU128,      // In fri.
    eth_l1_data_gas_price: NonZeroU128,  // In wei.
    strk_l1_data_gas_price: NonZeroU128, // In fri.
    eth_l2_gas_price: NonZeroU128,       // In wei.
    strk_l2_gas_price: NonZeroU128,      // In fri.
}

impl GasPrices {
    pub fn new(
        eth_l1_gas_price: NonZeroU128,
        strk_l1_gas_price: NonZeroU128,
        eth_l1_data_gas_price: NonZeroU128,
        strk_l1_data_gas_price: NonZeroU128,
    ) -> Self {
        // TODO(Aner): get gas prices from python.
        let eth_l2_gas_price = NonZeroU128::new(
            VersionedConstants::get_versioned_constants(VersionedConstantsOverrides {
                validate_max_n_steps: 0,
                max_recursion_depth: 0,
                versioned_constants_base_overrides: None,
            })
            .l1_to_l2_gas_price_conversion(eth_l1_gas_price.into()),
        )
        .expect("L1 to L2 price conversion error (Rust side).");
        let strk_l2_gas_price = NonZeroU128::new(
            VersionedConstants::get_versioned_constants(VersionedConstantsOverrides {
                validate_max_n_steps: 0,
                max_recursion_depth: 0,
                versioned_constants_base_overrides: None,
            })
            .l1_to_l2_gas_price_conversion(strk_l1_gas_price.into()),
        )
        .expect("L1 to L2 price conversion error (Rust side).");
        GasPrices {
            eth_l1_gas_price,
            strk_l1_gas_price,
            eth_l1_data_gas_price,
            strk_l1_data_gas_price,
            eth_l2_gas_price,
            strk_l2_gas_price,
        }
    }

    pub fn get_l1_gas_price_by_fee_type(&self, fee_type: &FeeType) -> NonZeroU128 {
        match fee_type {
            FeeType::Strk => self.strk_l1_gas_price,
            FeeType::Eth => self.eth_l1_gas_price,
        }
    }

    pub fn get_l1_data_gas_price_by_fee_type(&self, fee_type: &FeeType) -> NonZeroU128 {
        match fee_type {
            FeeType::Strk => self.strk_l1_data_gas_price,
            FeeType::Eth => self.eth_l1_data_gas_price,
        }
    }

    pub fn get_l2_gas_price_by_fee_type(&self, fee_type: &FeeType) -> NonZeroU128 {
        match fee_type {
            FeeType::Strk => self.strk_l2_gas_price,
            FeeType::Eth => self.eth_l2_gas_price,
        }
    }
}

// Block pre-processing.
// Writes the hash of the (current_block_number - N) block under its block number in the dedicated
// contract state, where N=STORED_BLOCK_HASH_BUFFER.
// NOTE: This function must remain idempotent since full nodes can call it for an already updated
// block hash table.
pub fn pre_process_block(
    state: &mut dyn State,
    old_block_number_and_hash: Option<BlockNumberHashPair>,
    next_block_number: BlockNumber,
) -> StateResult<()> {
    let should_block_hash_be_provided =
        next_block_number >= BlockNumber(constants::STORED_BLOCK_HASH_BUFFER);
    if let Some(BlockNumberHashPair { number: block_number, hash: block_hash }) =
        old_block_number_and_hash
    {
        let block_hash_contract_address =
            ContractAddress::from(constants::BLOCK_HASH_CONTRACT_ADDRESS);
        let block_number_as_storage_key = StorageKey::from(block_number.0);
        state.set_storage_at(
            block_hash_contract_address,
            block_number_as_storage_key,
            block_hash.0,
        )?;
    } else if should_block_hash_be_provided {
        return Err(StateError::OldBlockHashNotProvided);
    }

    Ok(())
}

pub struct BlockNumberHashPair {
    pub number: BlockNumber,
    pub hash: BlockHash,
}

impl BlockNumberHashPair {
    pub fn new(block_number: u64, block_hash: Felt) -> BlockNumberHashPair {
        BlockNumberHashPair { number: BlockNumber(block_number), hash: BlockHash(block_hash) }
    }
}
