// Execution constraints for transaction execution.

from starkware.cairo.common.bool import FALSE, TRUE
from starkware.cairo.common.dict_access import DictAccess
from starkware.cairo.common.math import assert_le, assert_nn_le, assert_not_zero
from starkware.starknet.core.os.constants import (
    ALLOWED_VIRTUAL_OS_PROGRAM_HASHES_0,
    ALLOWED_VIRTUAL_OS_PROGRAM_HASHES_LEN,
    STORED_BLOCK_HASH_BUFFER,
)
from starkware.starknet.core.os.execution.syscall_impls import read_block_hash_from_storage
from starkware.starknet.core.os.virtual_os_output import (
    PROOF_VERSION,
    VIRTUAL_OS_OUTPUT_VERSION,
    VIRTUAL_SNOS,
    VirtualOsOutputHeader,
)

func check_is_reverted(is_reverted: felt) {
    return ();
}

// Returns TRUE if the given virtual OS program hash is allowed, FALSE otherwise.
func is_program_hash_allowed(program_hash: felt) -> felt {
    static_assert ALLOWED_VIRTUAL_OS_PROGRAM_HASHES_LEN == 1;
    if (program_hash == ALLOWED_VIRTUAL_OS_PROGRAM_HASHES_0) {
        return TRUE;
    }
    return FALSE;
}

// Validates that the proof facts of an invoke transaction are of a valid virtual OS run.
func check_proof_facts{range_check_ptr, contract_state_changes: DictAccess*}(
    proof_facts_size: felt,
    proof_facts: felt*,
    current_block_number: felt,
    virtual_os_config_hash: felt,
) {
    if (proof_facts_size == 0) {
        return ();
    }
    alloc_locals;
    assert_le(VirtualOsOutputHeader.SIZE + 3, proof_facts_size);
    let proof_version = proof_facts[0];
    assert proof_version = PROOF_VERSION;
    let proof_type = proof_facts[1];
    assert proof_type = VIRTUAL_SNOS;
    let program_hash = proof_facts[2];
    assert is_program_hash_allowed(program_hash) = TRUE;
    let os_output_header = cast(&proof_facts[3], VirtualOsOutputHeader*);

    with_attr error_message("Virtual OS output version is not supported") {
        assert os_output_header.output_version = VIRTUAL_OS_OUTPUT_VERSION;
    }

    // Validate that the proof facts block number is not too recent.
    // (This is a sanity check - the following non-zero check ensures that the block hash is
    // not trivial).
    assert_nn_le(
        os_output_header.base_block_number, current_block_number - STORED_BLOCK_HASH_BUFFER
    );
    // Not all block hashes are stored in the contract; Make sure the requested one is not trivial.
    assert_not_zero(os_output_header.base_block_hash);

    // validate that the proof facts block hash is the true hash of the proof facts block number.
    read_block_hash_from_storage(
        block_number=os_output_header.base_block_number,
        expected_block_hash=os_output_header.base_block_hash,
    );

    // validate that the proof facts config hash is the true hash of the OS config.
    assert os_output_header.starknet_os_config_hash = virtual_os_config_hash;

    return ();
}
