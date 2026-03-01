from starkware.cairo.common.cairo_secp.constants import SECP_PRIME_HIGH as SECP256K1_PRIME_HIGH
from starkware.cairo.common.cairo_secp.constants import SECP_PRIME_LOW as SECP256K1_PRIME_LOW
from starkware.cairo.common.dict_access import DictAccess
from starkware.cairo.common.secp256r1.constants import SECP_PRIME_HIGH as SECP256R1_PRIME_HIGH
from starkware.cairo.common.secp256r1.constants import SECP_PRIME_LOW as SECP256R1_PRIME_LOW
from starkware.cairo.common.uint256 import Uint256
from starkware.starknet.common.new_syscalls import (
    CALL_CONTRACT_SELECTOR,
    EMIT_EVENT_SELECTOR,
    GET_CLASS_HASH_AT_SELECTOR,
    GET_EXECUTION_INFO_SELECTOR,
    LIBRARY_CALL_SELECTOR,
    SECP256K1_ADD_SELECTOR,
    SECP256K1_GET_POINT_FROM_X_SELECTOR,
    SECP256K1_GET_XY_SELECTOR,
    SECP256K1_MUL_SELECTOR,
    SECP256K1_NEW_SELECTOR,
    SECP256R1_ADD_SELECTOR,
    SECP256R1_GET_POINT_FROM_X_SELECTOR,
    SECP256R1_GET_XY_SELECTOR,
    SECP256R1_MUL_SELECTOR,
    SECP256R1_NEW_SELECTOR,
    SEND_MESSAGE_TO_L1_SELECTOR,
    SHA256_PROCESS_BLOCK_SELECTOR,
    STORAGE_READ_SELECTOR,
    STORAGE_WRITE_SELECTOR,
    EmitEventRequest,
)
from starkware.starknet.core.os.block_context import BlockContext
from starkware.starknet.core.os.builtins import BuiltinPointers
from starkware.starknet.core.os.constants import (
    EMIT_EVENT_GAS_COST,
    SECP256K1_GET_XY_GAS_COST,
    SECP256R1_GET_XY_GAS_COST,
)
from starkware.starknet.core.os.execution.execute_entry_point import ExecutionContext
from starkware.starknet.core.os.execution.revert import RevertLogEntry
from starkware.starknet.core.os.execution.syscall_impls import (
    execute_call_contract,
    execute_get_class_hash_at,
    execute_get_execution_info,
    execute_library_call,
    execute_secp256k1_add,
    execute_secp256k1_get_point_from_x,
    execute_secp256k1_mul,
    execute_secp256k1_new,
    execute_secp256r1_add,
    execute_secp256r1_get_point_from_x,
    execute_secp256r1_mul,
    execute_secp256r1_new,
    execute_secp_get_xy,
    execute_send_message_to_l1,
    execute_sha256_process_block,
    execute_storage_read,
    execute_storage_write,
    reduce_syscall_gas_and_write_response_header,
)
from starkware.starknet.core.os.output import OsCarriedOutputs

// Virtual OS version of execute_syscalls.
// Executes a subset of the system calls that are allowed in virtual OS mode.
func execute_syscalls{
    range_check_ptr,
    syscall_ptr: felt*,
    builtin_ptrs: BuiltinPointers*,
    contract_state_changes: DictAccess*,
    contract_class_changes: DictAccess*,
    revert_log: RevertLogEntry*,
    outputs: OsCarriedOutputs*,
}(block_context: BlockContext*, execution_context: ExecutionContext*, syscall_ptr_end: felt*) {
    alloc_locals;
    if (syscall_ptr == syscall_ptr_end) {
        return ();
    }

    local selector = [syscall_ptr];
    %{ LogEnterSyscall %}

    if (selector == STORAGE_READ_SELECTOR) {
        execute_storage_read(contract_address=execution_context.execution_info.contract_address);
        %{ OsLoggerExitSyscall %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == STORAGE_WRITE_SELECTOR) {
        execute_storage_write(contract_address=execution_context.execution_info.contract_address);
        %{ OsLoggerExitSyscall %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == GET_EXECUTION_INFO_SELECTOR) {
        execute_get_execution_info(execution_context=execution_context);
        %{ OsLoggerExitSyscall %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == CALL_CONTRACT_SELECTOR) {
        execute_call_contract(
            block_context=block_context, caller_execution_context=execution_context
        );
        %{ OsLoggerExitSyscall %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == LIBRARY_CALL_SELECTOR) {
        execute_library_call(
            block_context=block_context, caller_execution_context=execution_context
        );
        %{ OsLoggerExitSyscall %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == EMIT_EVENT_SELECTOR) {
        // Skip as long as the block hash is not calculated by the OS.
        // TODO(Yoni, 1/4/2022): calculate event hash in the OS.
        reduce_syscall_gas_and_write_response_header(
            total_gas_cost=EMIT_EVENT_GAS_COST, request_struct_size=EmitEventRequest.SIZE
        );
        %{ OsLoggerExitSyscall %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == GET_CLASS_HASH_AT_SELECTOR) {
        execute_get_class_hash_at();
        %{ OsLoggerExitSyscall %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SHA256_PROCESS_BLOCK_SELECTOR) {
        execute_sha256_process_block();
        %{ OsLoggerExitSyscall %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256K1_GET_POINT_FROM_X_SELECTOR) {
        execute_secp256k1_get_point_from_x();
        %{ OsLoggerExitSyscall %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256R1_GET_POINT_FROM_X_SELECTOR) {
        execute_secp256r1_get_point_from_x();
        %{ OsLoggerExitSyscall %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256K1_NEW_SELECTOR) {
        execute_secp256k1_new();
        %{ OsLoggerExitSyscall %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256R1_NEW_SELECTOR) {
        execute_secp256r1_new();
        %{ OsLoggerExitSyscall %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256K1_ADD_SELECTOR) {
        execute_secp256k1_add();
        %{ OsLoggerExitSyscall %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256R1_ADD_SELECTOR) {
        execute_secp256r1_add();
        %{ OsLoggerExitSyscall %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256K1_MUL_SELECTOR) {
        execute_secp256k1_mul();
        %{ OsLoggerExitSyscall %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256R1_MUL_SELECTOR) {
        execute_secp256r1_mul();
        %{ OsLoggerExitSyscall %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256K1_GET_XY_SELECTOR) {
        execute_secp_get_xy(
            curve_prime=Uint256(low=SECP256K1_PRIME_LOW, high=SECP256K1_PRIME_HIGH),
            gas_cost=SECP256K1_GET_XY_GAS_COST,
        );
        %{ OsLoggerExitSyscall %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256R1_GET_XY_SELECTOR) {
        execute_secp_get_xy(
            curve_prime=Uint256(low=SECP256R1_PRIME_LOW, high=SECP256R1_PRIME_HIGH),
            gas_cost=SECP256R1_GET_XY_GAS_COST,
        );
        %{ OsLoggerExitSyscall %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    with_attr error_message("Unexpected syscall selector in virtual mode: {selector}.") {
        assert selector = SEND_MESSAGE_TO_L1_SELECTOR;
    }
    execute_send_message_to_l1(contract_address=execution_context.execution_info.contract_address);
    %{ OsLoggerExitSyscall %}
    return execute_syscalls(
        block_context=block_context,
        execution_context=execution_context,
        syscall_ptr_end=syscall_ptr_end,
    );
}
