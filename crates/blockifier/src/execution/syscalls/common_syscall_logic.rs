use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_types_core::felt::Felt;

use crate::execution::syscalls::hint_processor::{INVALID_INPUT_LENGTH_ERROR, OUT_OF_GAS_ERROR};
use crate::execution::syscalls::syscall_base::KECCAK_FULL_RATE_IN_WORDS;
use crate::execution::syscalls::vm_syscall_utils::{
    handle_failure,
    SyscallBaseResult,
    SyscallExecutorBaseError,
};

#[allow(clippy::result_large_err)]
pub fn base_keccak(
    keccak_round_cost_base_syscall_cost: u64,
    input: &[u64],
    remaining_gas: &mut u64,
    vm: &mut VirtualMachine,
) -> SyscallBaseResult<([u64; 4], usize)> {
    let input_length = input.len();

    let (n_rounds, remainder) = num_integer::div_rem(input_length, KECCAK_FULL_RATE_IN_WORDS);

    if remainder != 0 {
        let revert_data = handle_failure(
            vm,
            Felt::from_hex(INVALID_INPUT_LENGTH_ERROR)
                .expect("Failed to parse INVALID_INPUT_LENGTH_ERROR hex string"),
        )?;
        return Err(SyscallExecutorBaseError::Revert(revert_data));
    }
    // TODO(Ori, 1/2/2024): Write an indicative expect message explaining why the conversion
    // works.
    let n_rounds_as_u64 = u64::try_from(n_rounds).expect("Failed to convert usize to u64.");
    let gas_cost = n_rounds_as_u64 * keccak_round_cost_base_syscall_cost;

    if gas_cost > *remaining_gas {
        let out_of_gas_error =
            Felt::from_hex(OUT_OF_GAS_ERROR).expect("Failed to parse OUT_OF_GAS_ERROR hex string");
        let revert_data = handle_failure(vm, out_of_gas_error)?;
        return Err(SyscallExecutorBaseError::Revert(revert_data));
    }
    *remaining_gas -= gas_cost;

    let mut state = [0u64; 25];
    for chunk in input.chunks(KECCAK_FULL_RATE_IN_WORDS) {
        for (i, val) in chunk.iter().enumerate() {
            state[i] ^= val;
        }
        keccak::f1600(&mut state)
    }

    Ok((state[..4].try_into().expect("Slice with incorrect length"), n_rounds))
}
