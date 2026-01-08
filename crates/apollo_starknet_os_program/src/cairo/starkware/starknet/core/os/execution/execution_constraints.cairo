// Execution constraints for transaction execution.

from starkware.cairo.common.dict import dict_read, dict_update
from starkware.cairo.common.dict_access import DictAccess
from starkware.cairo.common.math import assert_le, assert_not_zero
from starkware.starknet.core.os.block_context import BlockContext
from starkware.starknet.core.os.constants import (
    BLOCK_HASH_CONTRACT_ADDRESS,
    STORED_BLOCK_HASH_BUFFER,
)
from starkware.starknet.core.os.state.commitment import StateEntry

// These are no-op implementations for the Sequencer OS.

func check_n_txs(n_txs: felt) {
    return ();
}

func check_tx_type(tx_type: felt) {
    return ();
}

func check_sender_address(sender_address: felt, block_context: BlockContext*) {
    return ();
}

// Validates the proof facts of an invoke transaction.
func check_proof_facts{range_check_ptr, contract_state_changes: DictAccess*}(
    proof_facts_size: felt, proof_facts: felt*, current_block_number: felt
) {
    alloc_locals;
    if (proof_facts_size == 0) {
        return ();
    }
    assert_le(5, proof_facts_size);

    local proof_facts_block_number = [proof_facts + 2];
    local proof_facts_block_hash = [proof_facts + 3];

    assert_not_zero(proof_facts_block_hash);
    assert_le(proof_facts_block_number, current_block_number - STORED_BLOCK_HASH_BUFFER);

    // Debug hint: assert the block hash matches what's in storage.
    // This provides a clear error message if mismatch, rather than failing later in state update.
    %{ AssertProofFactsBlockHash %}

    // Get the block hash contract state entry.
    let (state_entry: StateEntry*) = dict_read{dict_ptr=contract_state_changes}(
        key=BLOCK_HASH_CONTRACT_ADDRESS
    );

    // Read from storage - assert the stored block hash matches.
    tempvar storage_ptr = state_entry.storage_ptr;
    assert [storage_ptr] = DictAccess(
        key=proof_facts_block_number,
        prev_value=proof_facts_block_hash,
        new_value=proof_facts_block_hash,
    );
    let storage_ptr = storage_ptr + DictAccess.SIZE;

    // Update the state with the new storage_ptr.
    dict_update{dict_ptr=contract_state_changes}(
        key=BLOCK_HASH_CONTRACT_ADDRESS,
        prev_value=cast(state_entry, felt),
        new_value=cast(
            new StateEntry(
                class_hash=state_entry.class_hash, storage_ptr=storage_ptr, nonce=state_entry.nonce
            ),
            felt,
        ),
    );

    return ();
}
