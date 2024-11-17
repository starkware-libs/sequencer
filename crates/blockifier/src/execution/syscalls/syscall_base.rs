use starknet_api::core::ContractAddress;
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::abi::constants;
use crate::execution::syscalls::hint_processor::{
    SyscallExecutionError,
    BLOCK_NUMBER_OUT_OF_RANGE_ERROR,
};

pub type SyscallResult<T> = Result<T, SyscallExecutionError>;

pub fn get_block_hash_base(
    current_block_number: u64,
    requested_block_number: u64,
) -> SyscallResult<(StorageKey, ContractAddress)> {
    if current_block_number < constants::STORED_BLOCK_HASH_BUFFER
        || requested_block_number > current_block_number - constants::STORED_BLOCK_HASH_BUFFER
    {
        let out_of_range_error =
            Felt::from_hex(BLOCK_NUMBER_OUT_OF_RANGE_ERROR).map_err(SyscallExecutionError::from)?;
        return Err(SyscallExecutionError::SyscallError { error_data: vec![out_of_range_error] });
    }

    let key = StorageKey::try_from(Felt::from(requested_block_number))?;
    let block_hash_contract_address =
        ContractAddress::try_from(Felt::from(constants::BLOCK_HASH_CONTRACT_ADDRESS))?;
    Ok((key, block_hash_contract_address))
}
