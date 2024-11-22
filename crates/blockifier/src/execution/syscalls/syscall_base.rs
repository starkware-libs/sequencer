/// This file is for sharing common logic between Native and Casm syscalls implementations.
use starknet_api::core::ContractAddress;
use starknet_api::state::StorageKey;
use starknet_types_core::felt::Felt;

use crate::abi::constants;
use crate::execution::common_hints::ExecutionMode;
use crate::execution::entry_point::EntryPointExecutionContext;
use crate::execution::syscalls::hint_processor::{
    SyscallExecutionError,
    BLOCK_NUMBER_OUT_OF_RANGE_ERROR,
    INVALID_INPUT_LENGTH_ERROR,
    OUT_OF_GAS_ERROR,
};
use crate::state::state_api::State;

pub type SyscallResult<T> = Result<T, SyscallExecutionError>;
pub const KECCAK_FULL_RATE_IN_WORDS: usize = 17;

pub fn get_block_hash_base(
    context: &EntryPointExecutionContext,
    requested_block_number: u64,
    state: &dyn State,
) -> SyscallResult<Felt> {
    let execution_mode = context.execution_mode;
    if execution_mode == ExecutionMode::Validate {
        return Err(SyscallExecutionError::InvalidSyscallInExecutionMode {
            syscall_name: "get_block_hash".to_string(),
            execution_mode,
        });
    }

    let current_block_number = context.tx_context.block_context.block_info.block_number.0;

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

pub fn keccak_base(
    context: &EntryPointExecutionContext,
    input_length: usize,
    remaining_gas: &mut u64,
) -> SyscallResult<usize> {
    let (n_rounds, remainder) = num_integer::div_rem(input_length, KECCAK_FULL_RATE_IN_WORDS);

    if remainder != 0 {
        return Err(SyscallExecutionError::SyscallError {
            error_data: vec![
                Felt::from_hex(INVALID_INPUT_LENGTH_ERROR).map_err(SyscallExecutionError::from)?,
            ],
        });
    }
    // TODO(Ori, 1/2/2024): Write an indicative expect message explaining why the conversion
    // works.
    let n_rounds_as_u64 = u64::try_from(n_rounds).expect("Failed to convert usize to u64.");
    let gas_cost = n_rounds_as_u64 * context.gas_costs().keccak_round_cost_gas_cost;

    if gas_cost > *remaining_gas {
        let out_of_gas_error =
            Felt::from_hex(OUT_OF_GAS_ERROR).map_err(SyscallExecutionError::from)?;

        return Err(SyscallExecutionError::SyscallError { error_data: vec![out_of_gas_error] });
    }
    *remaining_gas -= gas_cost;

    Ok(n_rounds)
}
