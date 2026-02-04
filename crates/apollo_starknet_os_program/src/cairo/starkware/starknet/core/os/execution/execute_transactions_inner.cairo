from starkware.cairo.common.dict_access import DictAccess
from starkware.starknet.core.os.block_context import BlockContext
from starkware.starknet.core.os.builtins import BuiltinPointers
from starkware.starknet.core.os.execution.transaction_impls import (
    execute_declare_transaction,
    execute_deploy_account_transaction,
    execute_invoke_function_transaction,
    execute_l1_handler_transaction,
)
from starkware.starknet.core.os.output import OsCarriedOutputs

// Inner function for execute_transactions.
// Arguments:
// block_context - a read-only context used for transaction execution.
// n_txs - the number of transactions to execute.
//
// Implicit arguments:
// range_check_ptr - a range check builtin, used and advanced by the OS, not the transactions.
// builtin_ptrs - a struct of builtin pointer that are going to be used by the
// executed transactions.
// The range-checks used internally by the transactions do not affect range_check_ptr.
// They are accounted for in builtin_ptrs.
func execute_transactions_inner{
    range_check_ptr,
    builtin_ptrs: BuiltinPointers*,
    contract_state_changes: DictAccess*,
    contract_class_changes: DictAccess*,
    outputs: OsCarriedOutputs*,
}(block_context: BlockContext*, n_txs) {
    %{ LogRemainingTxs %}
    if (n_txs == 0) {
        return ();
    }

    tempvar tx_type;
    // Guess the current transaction's type.
    %{ LoadNextTx %}

    if (tx_type == 'INVOKE_FUNCTION') {
        // Handle the invoke-function transaction.
        execute_invoke_function_transaction(block_context=block_context);
        %{ ExitTx %}
        return execute_transactions_inner(block_context=block_context, n_txs=n_txs - 1);
    }
    if (tx_type == 'L1_HANDLER') {
        // Handle the L1-handler transaction.
        execute_l1_handler_transaction(block_context=block_context);
        %{ ExitTx %}
        return execute_transactions_inner(block_context=block_context, n_txs=n_txs - 1);
    }
    if (tx_type == 'DEPLOY_ACCOUNT') {
        // Handle the deploy-account transaction.
        execute_deploy_account_transaction(block_context=block_context);
        %{ ExitTx %}
        return execute_transactions_inner(block_context=block_context, n_txs=n_txs - 1);
    }

    assert tx_type = 'DECLARE';
    // Handle the declare transaction.
    execute_declare_transaction(block_context=block_context);
    %{ ExitTx %}
    return execute_transactions_inner(block_context=block_context, n_txs=n_txs - 1);
}
