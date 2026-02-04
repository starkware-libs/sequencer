// Virtual OS version of execute_transactions_inner.cairo

from starkware.cairo.common.dict_access import DictAccess
from starkware.starknet.core.os.block_context import BlockContext
from starkware.starknet.core.os.builtins import BuiltinPointers
from starkware.starknet.core.os.execution.transaction_impls import (
    execute_invoke_function_transaction,
)
from starkware.starknet.core.os.output import OsCarriedOutputs

// In virtual OS mode, we only support a single INVOKE_FUNCTION transaction.
func execute_transactions_inner{
    range_check_ptr,
    builtin_ptrs: BuiltinPointers*,
    contract_state_changes: DictAccess*,
    contract_class_changes: DictAccess*,
    outputs: OsCarriedOutputs*,
}(block_context: BlockContext*, n_txs) {
    with_attr error_message("Expected exactly one transaction") {
        assert n_txs = 1;
    }

    tempvar tx_type;
    %{ LoadNextTx %}

    with_attr error_message("Expected INVOKE_FUNCTION transaction") {
        assert tx_type = 'INVOKE_FUNCTION';
    }
    execute_invoke_function_transaction(block_context=block_context);
    %{ ExitTx %}
    return ();
}
