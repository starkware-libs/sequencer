from starkware.cairo.common.bool import FALSE
from starkware.cairo.common.dict_access import DictAccess
from starkware.cairo.common.registers import get_fp_and_pc
from starkware.starknet.core.os.block_context import BlockContext
from starkware.starknet.core.os.builtins import BuiltinPointers
from starkware.starknet.core.os.execution.deprecated_execute_entry_point import (
    deprecated_execute_entry_point,
)
from starkware.starknet.core.os.execution.execute_entry_point import (
    ExecutionContext,
    execute_entry_point,
)
from starkware.starknet.core.os.execution.revert import RevertLogEntry
from starkware.starknet.core.os.output import OsCarriedOutputs

    static_assert DeprecatedContractEntryPoint.selector == 0;
    let (entry_point_desc: DeprecatedContractEntryPoint*, success) = search_sorted_optimistic(
        array_ptr=cast(entry_points, felt*),
        elm_size=DeprecatedContractEntryPoint.SIZE,
        n_elms=n_entry_points,
        key=execution_context.execution_info.selector,
    );
    if (success != FALSE) {
        return (success=1, entry_point_offset=entry_point_desc.offset);
    }

    // If the selector was not found, check if we have a default entry point.
    if (n_entry_points != 0 and entry_points[0].selector == DEFAULT_ENTRY_POINT_SELECTOR) {
        return (success=1, entry_point_offset=entry_points[0].offset);
    }
    return (success=0, entry_point_offset=0);
}

// Performs a Cairo jump to the function 'execute_deprecated_syscalls'.
// This function's signature must match the signature of 'execute_deprecated_syscalls'.
func call_execute_deprecated_syscalls{
    range_check_ptr,
    builtin_ptrs: BuiltinPointers*,
    contract_state_changes: DictAccess*,
    contract_class_changes: DictAccess*,
    revert_log: RevertLogEntry*,
    outputs: OsCarriedOutputs*,
}(
    block_context: BlockContext*,
    execution_context: ExecutionContext*,
    syscall_size,
    syscall_ptr: felt*,
) {
    jmp abs block_context.execute_deprecated_syscalls_ptr;
}

// Executes an entry point in a contract.
// The contract entry point is selected based on execution_context.entry_point_type
// and execution_context.execution_info.selector.
//
// Arguments:
// block_context - a global context that is fixed throughout the block.
// execution_context - The context for the current execution.
func deprecated_execute_entry_point{
    range_check_ptr,
    builtin_ptrs: BuiltinPointers*,
    contract_state_changes: DictAccess*,
    contract_class_changes: DictAccess*,
    revert_log: RevertLogEntry*,
    outputs: OsCarriedOutputs*,
}(block_context: BlockContext*, execution_context: ExecutionContext*) -> (
    is_reverted: felt, retdata_size: felt, retdata: felt*
) {
    alloc_locals;

    // The key must be at offset 0.
    static_assert DeprecatedCompiledClassFact.hash == 0;
    let (compiled_class_fact: DeprecatedCompiledClassFact*) = find_element(
        array_ptr=block_context.deprecated_compiled_class_facts,
        elm_size=DeprecatedCompiledClassFact.SIZE,
        n_elms=block_context.n_deprecated_compiled_class_facts,
        key=execution_context.class_hash,
