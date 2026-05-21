from starkware.cairo.common.dict_access import DictAccess
from starkware.starknet.core.os.block_context import BlockContext
from starkware.starknet.core.os.builtins import BuiltinPointers
from starkware.starknet.core.os.execution.execute_entry_point import ExecutionContext
from starkware.starknet.core.os.execution.execute_syscalls import execute_syscalls
from starkware.starknet.core.os.execution.revert import RevertLogEntry
from starkware.starknet.core.os.output import OsCarriedOutputs

// Entry point. Wraps syscall execution with a transaction context (LoadNextTx / ExitTx)
// so that the OS logger captures per-syscall resource usage.
func measure_syscall_resources{
    range_check_ptr,
    syscall_ptr: felt*,
    builtin_ptrs: BuiltinPointers*,
    contract_state_changes: DictAccess*,
    contract_class_changes: DictAccess*,
    revert_log: RevertLogEntry*,
    outputs: OsCarriedOutputs*,
}(
    block_context: BlockContext*,
    execution_context: ExecutionContext*,
    syscall_ptr_end: felt*,
) {
    alloc_locals;
    local tx_type;
    %{ LoadNextTx %}
    execute_syscalls(
        block_context=block_context,
        execution_context=execution_context,
        syscall_ptr_end=syscall_ptr_end,
    );
    %{ ExitTx %}
    return ();
}
