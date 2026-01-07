// Virtual OS version of entry_point_utils.cairo

from starkware.cairo.common.bool import FALSE
from starkware.cairo.common.dict_access import DictAccess
from starkware.starknet.core.os.block_context import BlockContext
from starkware.starknet.core.os.builtins import BuiltinPointers
from starkware.starknet.core.os.execution.execute_entry_point import (
    ExecutionContext,
    execute_entry_point,
)
from starkware.starknet.core.os.execution.revert import RevertLogEntry
from starkware.starknet.core.os.output import OsCarriedOutputs

// In virtual OS mode, we only support Cairo 1 contracts.
// We also assume that the Cairo 1 contract is a Sierra 1.7.0+ contract, that fully supports
// Sierra gas mode.
func select_execute_entry_point_func{
    range_check_ptr,
    remaining_gas: felt,
    builtin_ptrs: BuiltinPointers*,
    contract_state_changes: DictAccess*,
    contract_class_changes: DictAccess*,
    revert_log: RevertLogEntry*,
    outputs: OsCarriedOutputs*,
}(block_context: BlockContext*, execution_context: ExecutionContext*) -> (
    is_reverted: felt, retdata_size: felt, retdata: felt*, is_deprecated: felt
) {
    %{ EnterCall %}

    let (is_reverted, retdata_size, retdata) = execute_entry_point(
        block_context=block_context, execution_context=execution_context
    );

    return (
        is_reverted=is_reverted, retdata_size=retdata_size, retdata=retdata, is_deprecated=FALSE
    );
}
