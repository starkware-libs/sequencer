use starknet_api::block::{BlockHashAndNumber, BlockNumber};
use starknet_api::state::StorageKey;

use crate::abi::constants;
use crate::blockifier_versioned_constants::OsConstants;
use crate::state::errors::StateError;
use crate::state::state_api::{State, StateResult};

#[cfg(test)]
#[path = "block_test.rs"]
pub mod block_test;

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
