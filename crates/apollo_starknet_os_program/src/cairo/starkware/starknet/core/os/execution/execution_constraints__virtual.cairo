// Execution constraints for transaction execution (virtual OS version).

from starkware.cairo.common.bool import FALSE
from starkware.cairo.common.cairo_builtins import HashBuiltin
from starkware.cairo.common.dict_access import DictAccess

// Checks that the transaction is not reverted.
func check_is_reverted(is_reverted: felt) {
    with_attr error_message("Reverted transactions are not supported in virtual OS mode") {
        assert is_reverted = FALSE;
    }
    return ();
}

func check_proof_facts{
    hash_ptr: HashBuiltin*, range_check_ptr, contract_state_changes: DictAccess*
}(
    proof_facts_size: felt,
    proof_facts: felt*,
    current_block_number: felt,
    chain_id: felt,
    fee_token_address: felt,
) {
    with_attr error_message("Proof facts are not supported in virtual OS mode") {
        assert proof_facts_size = 0;
    }
    return ();
}
