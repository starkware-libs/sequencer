use starknet_api::core::ContractAddress;
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::abi::constants;
use crate::execution::common_hints::ExecutionMode;
use crate::execution::syscalls::hint_processor::{
    SyscallExecutionError,
    BLOCK_NUMBER_OUT_OF_RANGE_ERROR,
};
use crate::state::state_api::State;

pub type SyscallResult<T> = Result<T, SyscallExecutionError>;

pub fn get_block_hash_base(
    execution_mode: ExecutionMode,
    current_block_number: u64,
    requested_block_number: u64,
    state: &dyn State,
) -> SyscallResult<Felt> {
    if execution_mode == ExecutionMode::Validate {
        return Err(SyscallExecutionError::InvalidSyscallInExecutionMode {
            syscall_name: "get_block_hash".to_string(),
            execution_mode,
        });
    }

    if current_block_number < constants::STORED_BLOCK_HASH_BUFFER
        || requested_block_number > current_block_number - constants::STORED_BLOCK_HASH_BUFFER
    {
        let out_of_range_error = Felt::from_hex(BLOCK_NUMBER_OUT_OF_RANGE_ERROR)
            .expect("Converting BLOCK_NUMBER_OUT_OF_RANGE_ERROR to Felt should not fail.");
        return Err(SyscallExecutionError::SyscallError { error_data: vec![out_of_range_error] });
    }

    let key = StorageKey::try_from(Felt::from(requested_block_number))?;
    let block_hash_contract_address =
        ContractAddress::try_from(Felt::from(constants::BLOCK_HASH_CONTRACT_ADDRESS))?;
    Ok(state.get_storage_at(block_hash_contract_address, key)?)
}
