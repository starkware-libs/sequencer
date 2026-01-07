from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.bool import FALSE
from starkware.cairo.common.dict_access import DictAccess
from starkware.starknet.core.os.block_context import BlockContext
from starkware.starknet.core.os.builtins import BuiltinPointers
from starkware.starknet.core.os.constants import DEFAULT_INITIAL_GAS_COST
from starkware.starknet.core.os.execution.deprecated_execute_entry_point import (
    deprecated_execute_entry_point,
)
from starkware.starknet.core.os.execution.execute_entry_point import (
    ExecutionContext,
    execute_entry_point,
)
from starkware.starknet.core.os.execution.revert import RevertLogEntry
from starkware.starknet.core.os.output import OsCarriedOutputs

// Selects execute_entry_point function according to the Cairo version of the entry point.
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
    alloc_locals;
    // TODO(Yoni): SIERRA_GAS_MODE - move back inside `execute_entry_point` functions.
    %{ EnterCall %}

    local is_deprecated;
    %{ CheckIsDeprecated %}
    // Note that the class_hash is validated in both the `if` and `else` cases, so a malicious
    // prover won't be able to produce a proof if guesses the wrong case.
    if (is_deprecated != FALSE) {
        let (is_reverted, retdata_size, retdata: felt*) = deprecated_execute_entry_point(
            block_context=block_context, execution_context=execution_context
        );
        return (
            is_reverted=is_reverted, retdata_size=retdata_size, retdata=retdata, is_deprecated=1
        );
    }

    // TODO(Yoni): SIERRA_GAS_MODE - remove once all Cairo 1 contracts run with Sierra gas mode.
    local caller_remaining_gas = remaining_gas;
    local is_sierra_gas_mode;
    %{ IsSierraGasMode %}
    if (is_sierra_gas_mode != FALSE) {
        tempvar inner_remaining_gas = remaining_gas;
    } else {
        // Run with high enough gas to avoid out-of-gas.
        tempvar inner_remaining_gas = DEFAULT_INITIAL_GAS_COST;
    }
    %{ DebugExpectedInitialGas %}

    let (is_reverted, retdata_size, retdata) = execute_entry_point{
        remaining_gas=inner_remaining_gas
    }(block_context=block_context, execution_context=execution_context);

    if (is_sierra_gas_mode != FALSE) {
        tempvar remaining_gas = inner_remaining_gas;
    } else {
        // Do not count Sierra gas for the caller in this case.
        tempvar remaining_gas = caller_remaining_gas;
    }
    return (is_reverted=is_reverted, retdata_size=retdata_size, retdata=retdata, is_deprecated=0);
}
