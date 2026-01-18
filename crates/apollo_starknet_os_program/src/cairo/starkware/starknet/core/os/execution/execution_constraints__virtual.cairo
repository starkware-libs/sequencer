// Execution constraints for transaction execution (virtual OS version).

from starkware.cairo.common.bool import FALSE
from starkware.cairo.common.dict_access import DictAccess
from starkware.starknet.core.os.block_context import BlockContext

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

// Checks that the transaction is not reverted.
func check_is_reverted(is_reverted: felt) {
    with_attr error_message("Reverted transactions are not supported in virtual OS mode") {
        assert is_reverted = FALSE;
    }
    return ();
}

func check_proof_facts{range_check_ptr, contract_state_changes: DictAccess*}(
    proof_facts_size: felt, proof_facts: felt*, current_block_number: felt
) {
    with_attr error_message("Proof facts are not supported in virtual OS mode") {
        assert proof_facts_size = 0;
    }
    return ();
}
