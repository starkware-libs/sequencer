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
    %{
        execution_helper.enter_call(
            cairo_execution_info=ids.execution_context.execution_info,
            deprecated_tx_info=ids.execution_context.deprecated_tx_info,
        )
    %}

    %{ is_deprecated = 1 if ids.execution_context.class_hash in __deprecated_class_hashes else 0 %}
    // Note that the class_hash is validated in both the `if` and `else` cases, so a malicious
    // prover won't be able to produce a proof if guesses the wrong case.
    if (nondet %{ is_deprecated %} != FALSE) {
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
    %{ ids.is_sierra_gas_mode = execution_helper.call_info.tracked_resource.is_sierra_gas() %}
    if (is_sierra_gas_mode != FALSE) {
        tempvar inner_remaining_gas = remaining_gas;
    } else {
        // Run with high enough gas to avoid out-of-gas.
        tempvar inner_remaining_gas = DEFAULT_INITIAL_GAS_COST;
    }
    %{
        if execution_helper.debug_mode:
            expected_initial_gas = execution_helper.call_info.call.initial_gas
            call_initial_gas = ids.inner_remaining_gas
            assert expected_initial_gas == call_initial_gas, (
                f"Expected remaining_gas {expected_initial_gas}. Got: {call_initial_gas}.\n"
                f"{execution_helper.call_info=}"
            )
    %}

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

// Same as `select_execute_entry_point_func`, but does not support reverts and does
// not have an implicit 'revert_log' argument.
