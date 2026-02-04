from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.cairo_builtins import (
    BitwiseBuiltin,
    EcOpBuiltin,
    HashBuiltin,
    KeccakBuiltin,
    ModBuiltin,
    PoseidonBuiltin,
)
from starkware.cairo.common.dict_access import DictAccess
from starkware.starknet.builtins.segment_arena.segment_arena import new_arena
from starkware.starknet.common.new_syscalls import ResourceBounds
from starkware.starknet.core.os.block_context import BlockContext
from starkware.starknet.core.os.builtins import (
    BuiltinPointers,
    NonSelectableBuiltins,
    SelectableBuiltins,
)
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
    %{ print(f"execute_transactions_inner: {ids.n_txs} transactions remaining.") %}
    if (n_txs == 0) {
        return ();
    }

    alloc_locals;
    local tx_type;
    local n_resource_bounds: felt;
    local resource_bounds: ResourceBounds*;

    // Guess the current transaction's type.
    %{
        tx = next(transactions)
        assert tx.tx_type.name in ('INVOKE_FUNCTION', 'L1_HANDLER', 'DEPLOY_ACCOUNT', 'DECLARE'), (
            f"Unexpected transaction type: {tx.type.name}."
        )

        tx_type_bytes = tx.tx_type.name.encode("ascii")
        ids.tx_type = int.from_bytes(tx_type_bytes, "big")
        execution_helper.os_logger.enter_tx(
            tx=tx,
            n_steps=current_step,
            builtin_ptrs=ids.builtin_ptrs,
            range_check_ptr=ids.range_check_ptr,
        )

        # Prepare a short callable to save code duplication.
        exit_tx = lambda: execution_helper.os_logger.exit_tx(
            n_steps=current_step,
            builtin_ptrs=ids.builtin_ptrs,
            range_check_ptr=ids.range_check_ptr,
        )
    %}

    if (tx_type == 'INVOKE_FUNCTION') {
        // Handle the invoke-function transaction.
        execute_invoke_function_transaction(block_context=block_context);
        %{ exit_tx() %}
        return execute_transactions_inner(block_context=block_context, n_txs=n_txs - 1);
    }
    if (tx_type == 'L1_HANDLER') {
        // Handle the L1-handler transaction.
        execute_l1_handler_transaction(block_context=block_context);
        %{ exit_tx() %}
        return execute_transactions_inner(block_context=block_context, n_txs=n_txs - 1);
    }
    if (tx_type == 'DEPLOY_ACCOUNT') {
        // Handle the deploy-account transaction.
        execute_deploy_account_transaction(block_context=block_context);
        %{ exit_tx() %}
        return execute_transactions_inner(block_context=block_context, n_txs=n_txs - 1);
    }

    assert tx_type = 'DECLARE';
    // Handle the declare transaction.
    execute_declare_transaction(block_context=block_context);
    %{ exit_tx() %}
    return execute_transactions_inner(block_context=block_context, n_txs=n_txs - 1);
}

