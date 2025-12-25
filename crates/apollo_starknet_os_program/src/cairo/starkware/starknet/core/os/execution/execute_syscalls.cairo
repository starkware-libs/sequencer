from starkware.cairo.common.bool import FALSE
from starkware.cairo.common.cairo_secp.constants import SECP_PRIME_HIGH as SECP256K1_PRIME_HIGH
from starkware.cairo.common.cairo_secp.constants import SECP_PRIME_LOW as SECP256K1_PRIME_LOW
from starkware.cairo.common.dict_access import DictAccess
from starkware.cairo.common.secp256r1.constants import SECP_PRIME_HIGH as SECP256R1_PRIME_HIGH
from starkware.cairo.common.secp256r1.constants import SECP_PRIME_LOW as SECP256R1_PRIME_LOW
from starkware.cairo.common.uint256 import Uint256
from starkware.starknet.common.new_syscalls import (
    CALL_CONTRACT_SELECTOR,
    DEPLOY_SELECTOR,
    EMIT_EVENT_SELECTOR,
    GET_BLOCK_HASH_SELECTOR,
    GET_CLASS_HASH_AT_SELECTOR,
    GET_EXECUTION_INFO_SELECTOR,
    KECCAK_SELECTOR,
    LIBRARY_CALL_SELECTOR,
    META_TX_V0_SELECTOR,
    REPLACE_CLASS_SELECTOR,
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
    execute_deploy,
    execute_get_block_hash,
    execute_get_class_hash_at,
    execute_get_execution_info,
    execute_keccak,
    execute_library_call,
    execute_meta_tx_v0,
    execute_replace_class,
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

// Executes the system calls in syscall_ptr.
// The signature of the function 'call_execute_syscalls' must match this function's signature.
//
// Arguments:
// block_context - a read-only context used for transaction execution.
// execution_context - The execution context in which the system calls need to be executed.
// syscall_ptr_end - a pointer to the end of the syscall segment.
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
    %{
        execution_helper.os_logger.enter_syscall(
            n_steps=current_step,
            builtin_ptrs=ids.builtin_ptrs,
            range_check_ptr=ids.range_check_ptr,
            deprecated=False,
            selector=ids.selector,
        )

        # Prepare a short callable to save code duplication.
        exit_syscall = lambda: execution_helper.os_logger.exit_syscall(
            n_steps=current_step,
            builtin_ptrs=ids.builtin_ptrs,
            range_check_ptr=ids.range_check_ptr,
            selector=ids.selector,
        )
    %}

    if (selector == STORAGE_READ_SELECTOR) {
        execute_storage_read(contract_address=execution_context.execution_info.contract_address);
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == STORAGE_WRITE_SELECTOR) {
        execute_storage_write(contract_address=execution_context.execution_info.contract_address);
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == GET_EXECUTION_INFO_SELECTOR) {
        execute_get_execution_info(execution_context=execution_context);
        %{ exit_syscall() %}
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
        %{ exit_syscall() %}
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
        %{ exit_syscall() %}
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
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == DEPLOY_SELECTOR) {
        execute_deploy(block_context=block_context, caller_execution_context=execution_context);
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == GET_BLOCK_HASH_SELECTOR) {
        execute_get_block_hash(block_context=block_context);
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == GET_CLASS_HASH_AT_SELECTOR) {
        execute_get_class_hash_at();
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == REPLACE_CLASS_SELECTOR) {
        execute_replace_class(contract_address=execution_context.execution_info.contract_address);
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == KECCAK_SELECTOR) {
        execute_keccak();
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SHA256_PROCESS_BLOCK_SELECTOR) {
        execute_sha256_process_block();
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256K1_GET_POINT_FROM_X_SELECTOR) {
        execute_secp256k1_get_point_from_x();
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256R1_GET_POINT_FROM_X_SELECTOR) {
        execute_secp256r1_get_point_from_x();
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256K1_NEW_SELECTOR) {
        execute_secp256k1_new();
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256R1_NEW_SELECTOR) {
        execute_secp256r1_new();
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256K1_ADD_SELECTOR) {
        execute_secp256k1_add();
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256R1_ADD_SELECTOR) {
        execute_secp256r1_add();
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256K1_MUL_SELECTOR) {
        execute_secp256k1_mul();
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256R1_MUL_SELECTOR) {
        execute_secp256r1_mul();
        %{ exit_syscall() %}
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
        %{ exit_syscall() %}
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
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SEND_MESSAGE_TO_L1_SELECTOR) {
        execute_send_message_to_l1(
            contract_address=execution_context.execution_info.contract_address
        );
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    assert selector = META_TX_V0_SELECTOR;
    execute_meta_tx_v0(block_context=block_context, caller_execution_context=execution_context);
    %{ exit_syscall() %}
    return execute_syscalls(
        block_context=block_context,
        execution_context=execution_context,
        syscall_ptr_end=syscall_ptr_end,
    );
}
