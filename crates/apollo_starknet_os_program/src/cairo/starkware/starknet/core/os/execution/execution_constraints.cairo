// Execution constraints for transaction execution.

from starkware.cairo.common.dict import dict_read, dict_update
from starkware.cairo.common.dict_access import DictAccess
from starkware.cairo.common.math import assert_le, assert_not_zero
from starkware.starknet.core.os.block_context import BlockContext
from starkware.starknet.core.os.constants import (
    BLOCK_HASH_CONTRACT_ADDRESS,
    STORED_BLOCK_HASH_BUFFER,
)
from starkware.starknet.core.os.execution.syscall_impls import read_block_hash_from_storage
from starkware.starknet.core.os.state.commitment import StateEntry
from starkware.starknet.core.os.virtual_os_output import VirtualOsOutputHeader

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
    if (proof_facts_size == 0) {
        return ();
    }
    alloc_locals;
    assert_le(VirtualOsOutputHeader.SIZE + 2, proof_facts_size);
    tempvar virtual_snos = [proof_facts];
    tempvar program_hash = [proof_facts + 1];

    tempvar os_output_header = cast(proof_facts + 2, VirtualOsOutputHeader*);
    assert_le(STORED_BLOCK_HASH_BUFFER, current_block_number);
    assert_le(os_output_header.prev_block_number, current_block_number - STORED_BLOCK_HASH_BUFFER);
    assert_not_zero(os_output_header.prev_block_hash);

    return ();
}
