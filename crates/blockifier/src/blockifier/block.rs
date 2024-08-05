use std::num::NonZeroU128;

use num_rational::Ratio;
use starknet_api::block::{BlockHash, BlockNumber, BlockTimestamp};
use starknet_api::core::ContractAddress;
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::abi::constants;
use crate::state::errors::StateError;
use crate::state::state_api::{State, StateResult};
use crate::transaction::objects::FeeType;

#[cfg(test)]
#[path = "block_test.rs"]
pub mod block_test;
pub const L2_GAS_FOR_CAIRO_STEP: u128 = 100;
pub const CAIRO_STEPS_PER_L1_GAS: u128 = 400;
pub const L2_TO_L1_GAS_PRICE_RATIO: u128 = L2_GAS_FOR_CAIRO_STEP * CAIRO_STEPS_PER_L1_GAS;

pub type L2Cost = Ratio<u128>;

#[derive(Clone, Debug)]
pub struct BlockInfo {
    pub block_number: BlockNumber,
    pub block_timestamp: BlockTimestamp,

    // Fee-related.
    pub sequencer_address: ContractAddress,
    pub gas_prices: GasPrices,
    pub use_kzg_da: bool,
}

#[derive(Clone, Debug)]
pub struct GasPrices {
    pub eth_l1_gas_price: NonZeroU128,       // In wei.
    pub strk_l1_gas_price: NonZeroU128,      // In fri.
    pub eth_l1_data_gas_price: NonZeroU128,  // In wei.
    pub strk_l1_data_gas_price: NonZeroU128, // In fri.
}

impl GasPrices {
    pub fn get_gas_price_by_fee_type(&self, fee_type: &FeeType) -> NonZeroU128 {
        match fee_type {
            FeeType::Strk => self.strk_l1_gas_price,
            FeeType::Eth => self.eth_l1_gas_price,
        }
    }

    pub fn get_data_gas_price_by_fee_type(&self, fee_type: &FeeType) -> NonZeroU128 {
        match fee_type {
            FeeType::Strk => self.strk_l1_data_gas_price,
            FeeType::Eth => self.eth_l1_data_gas_price,
        }
    }

    pub fn get_l2_gas_price_by_fee_type(&self, fee_type: &FeeType) -> L2Cost {
        L2Cost::new(self.get_gas_price_by_fee_type(fee_type).into(), L2_TO_L1_GAS_PRICE_RATIO)
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
