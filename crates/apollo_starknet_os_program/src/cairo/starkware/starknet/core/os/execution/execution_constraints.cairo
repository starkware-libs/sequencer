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

func check_is_reverted(is_reverted: felt) {
    return ();
}

// Validates that the proof facts of an invoke transaction are of a valid virtual OS run.
func check_proof_facts{range_check_ptr, contract_state_changes: DictAccess*}(
    proof_facts_size: felt, proof_facts: felt*, current_block_number: felt
) {
    if (proof_facts_size == 0) {
        return ();
    }
    alloc_locals;
    assert_le(VirtualOsOutputHeader.SIZE + 2, proof_facts_size);
    let proof_type = proof_facts[0];
    assert proof_type = 'VIRTUAL_SNOS';
    // TODO(Meshi): add a check that the program hash is the virtual OS program hash.
    let program_hash = proof_facts[1];

    let os_output_header = cast(&proof_facts[2], VirtualOsOutputHeader*);
    // validate that the proof facts block number is not too recent
    // (the first check is to avoid underflow in the second one).
    assert_le(STORED_BLOCK_HASH_BUFFER, current_block_number);
    assert_le(os_output_header.base_block_number, current_block_number - STORED_BLOCK_HASH_BUFFER);
    // Not all block hashes are stored in the contract; Make sure the requested one is not trivial.
    assert_not_zero(os_output_header.base_block_hash);

    // TODO(Meshi): add a better way to debug this failure.
    // validate that the proof facts block hash is the true hash of the proof facts block number.
    read_block_hash_from_storage(
        block_number=os_output_header.base_block_number,
        expected_block_hash=os_output_header.base_block_hash,
    );

    return ();
}
