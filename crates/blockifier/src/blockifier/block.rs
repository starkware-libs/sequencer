use log::warn;
use starknet_api::block::{
    BlockHashAndNumber,
    BlockNumber,
    GasPrice,
    GasPriceVector,
    GasPrices,
    NonzeroGasPrice,
};
use starknet_api::state::StorageKey;

use crate::abi::constants;
use crate::blockifier_versioned_constants::{OsConstants, VersionedConstants};
use crate::state::errors::StateError;
use crate::state::state_api::{State, StateResult};

#[cfg(test)]
#[path = "block_test.rs"]
pub mod block_test;

/// Warns if the submitted gas prices do not match the expected gas prices.
fn validate_l2_gas_price(gas_prices: &GasPrices) {
    // TODO(Aner): fix backwards compatibility.
    let eth_l2_gas_price = gas_prices.eth_gas_prices.l2_gas_price;
    let expected_eth_l2_gas_price = VersionedConstants::latest_constants()
        .convert_l1_to_l2_gas_price_round_up(gas_prices.eth_gas_prices.l1_gas_price.into());
    if GasPrice::from(eth_l2_gas_price) != expected_eth_l2_gas_price {
        // TODO(Aner): change to panic! Requires fixing several tests.
        warn!(
            "eth_l2_gas_price {eth_l2_gas_price} does not match expected eth_l2_gas_price \
             {expected_eth_l2_gas_price}."
        )
    }
    let strk_l2_gas_price = gas_prices.strk_gas_prices.l2_gas_price;
    let expected_strk_l2_gas_price = VersionedConstants::latest_constants()
        .convert_l1_to_l2_gas_price_round_up(gas_prices.strk_gas_prices.l1_gas_price.into());
    if GasPrice::from(strk_l2_gas_price) != expected_strk_l2_gas_price {
        // TODO(Aner): change to panic! Requires fixing test_discounted_gas_overdraft
        warn!(
            "strk_l2_gas_price {strk_l2_gas_price} does not match expected strk_l2_gas_price \
             {expected_strk_l2_gas_price}."
        )
    }
}

pub fn validated_gas_prices(
    eth_l1_gas_price: NonzeroGasPrice,
    strk_l1_gas_price: NonzeroGasPrice,
    eth_l1_data_gas_price: NonzeroGasPrice,
    strk_l1_data_gas_price: NonzeroGasPrice,
    eth_l2_gas_price: NonzeroGasPrice,
    strk_l2_gas_price: NonzeroGasPrice,
) -> GasPrices {
    let gas_prices = GasPrices {
        eth_gas_prices: GasPriceVector {
            l1_gas_price: eth_l1_gas_price,
            l1_data_gas_price: eth_l1_data_gas_price,
            l2_gas_price: eth_l2_gas_price,
        },
        strk_gas_prices: GasPriceVector {
            l1_gas_price: strk_l1_gas_price,
            l1_data_gas_price: strk_l1_data_gas_price,
            l2_gas_price: strk_l2_gas_price,
        },
    };
    validate_l2_gas_price(&gas_prices);

    gas_prices
}

// Block pre-processing.
// Writes the hash of the (current_block_number - N) block under its block number in the dedicated
// contract state, where N=STORED_BLOCK_HASH_BUFFER.
// NOTE: This function must remain idempotent since full nodes can call it for an already updated
// block hash table.
pub fn pre_process_block(
    state: &mut dyn State,
    old_block_number_and_hash: Option<BlockHashAndNumber>,
    next_block_number: BlockNumber,
    os_constants: &OsConstants,
) -> StateResult<()> {
    let should_block_hash_be_provided =
        next_block_number >= BlockNumber(constants::STORED_BLOCK_HASH_BUFFER);
    if let Some(BlockHashAndNumber { number, hash }) = old_block_number_and_hash {
        let block_hash_contract_address =
            os_constants.os_contract_addresses.block_hash_contract_address();
        let block_number_as_storage_key = StorageKey::from(number.0);
        state.set_storage_at(block_hash_contract_address, block_number_as_storage_key, hash.0)?;
    } else if should_block_hash_be_provided {
        return Err(StateError::OldBlockHashNotProvided);
    }

    Ok(())
}
