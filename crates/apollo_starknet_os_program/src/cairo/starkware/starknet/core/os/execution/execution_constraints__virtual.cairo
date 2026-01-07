// Execution constraints for transaction execution (virtual OS version).

from starkware.cairo.common.dict_access import DictAccess
from starkware.starknet.core.os.block_context import BlockContext
from starkware.starknet.core.os.execution.execute_entry_point import ExecutionContext

// Checks that the number of transactions is one.
func check_n_txs(n_txs: felt) {
    with_attr error_message("Expected exactly one transaction") {
        assert n_txs = 1;
    }
    return ();
}

// Checks that the transaction type is INVOKE_FUNCTION.
func check_tx_type(tx_type: felt) {
    with_attr error_message("Expected INVOKE_FUNCTION transaction") {
        assert tx_type = 'INVOKE_FUNCTION';
    }
    return ();
}

// Checks that the sender address matches the authorized account address.
func check_sender_address(sender_address: felt, block_context: BlockContext*) {
    let virtual_os_config = block_context.os_global_context.virtual_os_config;
    with_attr error_message("Sender address does not match authorized account address") {
        assert virtual_os_config.authorized_account_address = sender_address;
    }
    return ();
}

func check_proof_facts{range_check_ptr, contract_state_changes: DictAccess*}(
    block_context: BlockContext*,
    tx_execution_context: ExecutionContext*,
    proof_facts: felt*,
    proof_facts_size: felt,
) {
    return ();
}
