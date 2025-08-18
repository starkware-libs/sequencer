use crate::execution::syscalls::hint_processor::{
    INVALID_INPUT_LENGTH_ERROR_FELT,
    OUT_OF_GAS_ERROR_FELT,
};
use crate::execution::syscalls::syscall_base::KECCAK_FULL_RATE_IN_WORDS;
use crate::execution::syscalls::vm_syscall_utils::{SyscallBaseResult, SyscallExecutorBaseError};

pub fn base_keccak(
    keccak_round_cost_base_syscall_cost: u64,
    input: &[u64],
    remaining_gas: &mut u64,
) -> SyscallBaseResult<([u64; 4], usize)> {
    let input_length = input.len();

    let (n_rounds, remainder) = num_integer::div_rem(input_length, KECCAK_FULL_RATE_IN_WORDS);

    if remainder != 0 {
        return Err(SyscallExecutorBaseError::Revert {
            error_data: vec![INVALID_INPUT_LENGTH_ERROR_FELT],
        });
    }
    // TODO(Ori, 1/2/2024): Write an indicative expect message explaining why the conversion
    // works.
    let n_rounds_as_u64 = u64::try_from(n_rounds).expect("Failed to convert usize to u64.");
    let gas_cost = n_rounds_as_u64 * keccak_round_cost_base_syscall_cost;

    if gas_cost > *remaining_gas {
        let out_of_gas_error = OUT_OF_GAS_ERROR_FELT;

        return Err(SyscallExecutorBaseError::Revert { error_data: vec![out_of_gas_error] });
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
