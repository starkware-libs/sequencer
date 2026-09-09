from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.math import assert_le
from starkware.cairo.common.memcpy import memcpy
from starkware.starknet.core.os.output import (
    MessageToL1Header,
    MessageToL2Header,
    OsCarriedOutputs,
    OsOutput,
    OsOutputHeader,
)
from starkware.starknet.core.os.proof_fact_fold import (
    FOLD_ENTRY_N_WORDS,
    fold_block_root_entries,
    pack_output_digest,
    unpack_block_root_entry,
)
from starkware.starknet.core.os.state.commitment import CommitmentUpdate
from starkware.starknet.core.os.state.squash import squash_class_changes, squash_state_changes
from starkware.starknet.core.os.state.state import SquashedOsStateUpdate

// Copies the L1<>L2 message segments from `current` to the end of the aggregated values
// and returns the new aggregated values.
func copy_l1l2_messages(
    aggregated_carried_outputs: OsCarriedOutputs*, current: OsOutput*
) -> OsCarriedOutputs* {
    alloc_locals;

    local initial_messages_to_l1: felt* = current.initial_carried_outputs.messages_to_l1;
    local final_messages_to_l1: felt* = current.final_carried_outputs.messages_to_l1;
    local res_messages_to_l1: felt* = aggregated_carried_outputs.messages_to_l1;
    local len_messages_to_l1: felt = final_messages_to_l1 - initial_messages_to_l1;
    memcpy(dst=res_messages_to_l1, src=initial_messages_to_l1, len=len_messages_to_l1);
    local res_messages_to_l1: felt* = res_messages_to_l1 + len_messages_to_l1;

    local initial_messages_to_l2: felt* = current.initial_carried_outputs.messages_to_l2;
    local final_messages_to_l2: felt* = current.final_carried_outputs.messages_to_l2;
    local res_messages_to_l2: felt* = aggregated_carried_outputs.messages_to_l2;
    local len_messages_to_l2: felt = final_messages_to_l2 - initial_messages_to_l2;
    memcpy(dst=res_messages_to_l2, src=initial_messages_to_l2, len=len_messages_to_l2);
    local res_messages_to_l2: felt* = res_messages_to_l2 + len_messages_to_l2;

    tempvar res = new OsCarriedOutputs(
        messages_to_l1=cast(res_messages_to_l1, MessageToL1Header*),
        messages_to_l2=cast(res_messages_to_l2, MessageToL2Header*),
    );

    return res;
}

// Combines the outputs of multiple Starknet OS runs into a single output, by:
// * checking that the final values of one block match the initial values of the next block,
// * squashing the state updates,
// * concatenating the L1<>L2 message segments.
//
// `os_program_hash` is used for the `os_program_hash` field of the combined output.
func combine_blocks{range_check_ptr}(
    n: felt, os_outputs: OsOutput*, os_program_hash: felt, use_kzg_da: felt, full_output: felt
) -> OsOutput* {
    alloc_locals;

    // Assert that 1 <= n < 2**128 + 1.
    // This assertion is not essential but is here for explicitness.
    // It is not essential as if it doesn't hold, the program would never end anyway.
    assert_le(1, n);

    local initial_carried_outputs: OsCarriedOutputs*;

    %{ AllocateSegmentsForMessages %}

    let first = os_outputs[0];
    // Validate fields of the first inner OS outputs. It cannot be checked in the inner function
    // as we only have the aggregated version of the first block where these fields are not
    // taken from the OS outputs.
    tempvar first_header: OsOutputHeader* = first.header;
    assert first_header.use_kzg_da = 0;
    assert first_header.full_output = 1;
    assert first_header.os_program_hash = 0;

    // Copy the messages of the first block.
    let final_carried_outputs = copy_l1l2_messages(
        aggregated_carried_outputs=initial_carried_outputs, current=&first
    );

    // Verify that the guessed values are 0 or 1.
    assert use_kzg_da * use_kzg_da = use_kzg_da;
    assert full_output * full_output = full_output;

    // The proof-fact fold fields are over all blocks, so they are computed after the
    // linear pass (see `combine_proof_facts_folds`); the running values are unused.
    tempvar aggregated = new OsOutput(
        header=new OsOutputHeader(
            state_update_output=first.header.state_update_output,
            prev_block_number=first.header.prev_block_number,
            new_block_number=first.header.new_block_number,
            prev_block_hash=first.header.prev_block_hash,
            new_block_hash=first.header.new_block_hash,
            os_program_hash=os_program_hash,
            starknet_os_config_hash=first.header.starknet_os_config_hash,
            use_kzg_da=use_kzg_da,
            full_output=full_output,
            proof_facts_root_output_low=0,
            proof_facts_root_output_high=0,
            n_proof_facts_transactions=0,
        ),
        squashed_os_state_update=first.squashed_os_state_update,
        initial_carried_outputs=initial_carried_outputs,
        final_carried_outputs=final_carried_outputs,
    );

    let res = combine_blocks_inner(aggregated=aggregated, n=n - 1, os_outputs=&os_outputs[1]);
    local res_state_update: SquashedOsStateUpdate = [res.squashed_os_state_update];

    // Fold the blocks' proof-fact roots into the combined output's root output digest.
    let (
        local n_proof_facts_transactions,
        local proof_facts_root_output_low,
        local proof_facts_root_output_high,
    ) = combine_proof_facts_folds(n=n, os_outputs=os_outputs);
    local res_header: OsOutputHeader* = res.header;

    %{ SetStateUpdatePointersToNone %}

    // Squash the contract state diff dict.
    let (n_contract_state_changes, squashed_contract_state_dict) = squash_state_changes(
        contract_state_changes_start=res_state_update.contract_state_changes,
        contract_state_changes_end=&res_state_update.contract_state_changes[
            res_state_update.n_contract_state_changes
        ],
    );

    // Squash the contract class diff dict.
    let (n_class_updates, squashed_class_changes) = squash_class_changes(
        class_changes_start=res_state_update.contract_class_changes,
        class_changes_end=&res_state_update.contract_class_changes[
            res_state_update.n_class_updates
        ],
    );

    tempvar squashed_res = new OsOutput(
        header=new OsOutputHeader(
            state_update_output=res_header.state_update_output,
            prev_block_number=res_header.prev_block_number,
            new_block_number=res_header.new_block_number,
            prev_block_hash=res_header.prev_block_hash,
            new_block_hash=res_header.new_block_hash,
            os_program_hash=res_header.os_program_hash,
            starknet_os_config_hash=res_header.starknet_os_config_hash,
            use_kzg_da=res_header.use_kzg_da,
            full_output=res_header.full_output,
            proof_facts_root_output_low=proof_facts_root_output_low,
            proof_facts_root_output_high=proof_facts_root_output_high,
            n_proof_facts_transactions=n_proof_facts_transactions,
        ),
        squashed_os_state_update=new SquashedOsStateUpdate(
            contract_state_changes=squashed_contract_state_dict,
            n_contract_state_changes=n_contract_state_changes,
            contract_class_changes=squashed_class_changes,
            n_class_updates=n_class_updates,
        ),
        initial_carried_outputs=res.initial_carried_outputs,
        final_carried_outputs=res.final_carried_outputs,
    );

    return squashed_res;
}

// Helper function for `combine_blocks`.
func combine_blocks_inner(aggregated: OsOutput*, n: felt, os_outputs: OsOutput*) -> OsOutput* {
    if (n == 0) {
        return aggregated;
    }

    alloc_locals;

    let current = os_outputs[0];
    tempvar current_header: OsOutputHeader* = current.header;
    tempvar aggregated_header: OsOutputHeader* = aggregated.header;

    // Check the size of `OsOutput` and `OsOutputHeader` to ensure that if new fields are added
    // they are handled by the aggregator.
    static_assert OsOutput.SIZE == 4;
    static_assert OsOutputHeader.SIZE == 12;

    // Validate fields of the inner OS output of a single task.
    assert current_header.use_kzg_da = 0;
    assert current_header.full_output = 1;
    assert current_header.os_program_hash = 0;

    // Check header consistency.
    assert current_header.state_update_output.initial_root = (
        aggregated_header.state_update_output.final_root
    );
    assert current_header.prev_block_number = aggregated_header.new_block_number;
    assert current_header.prev_block_hash = aggregated_header.new_block_hash;
    assert current_header.starknet_os_config_hash = aggregated_header.starknet_os_config_hash;

    // Check `squashed_os_state_update` consistency: the dictionary entries of the blocks must form
    // one contiguous segment (this is done as part of the hint generating them). Check that the
    // beginning of the current block is at the end of the blocks aggregated so far.
    local aggregated_update: SquashedOsStateUpdate = [aggregated.squashed_os_state_update];
    local current_update: SquashedOsStateUpdate = [current.squashed_os_state_update];
    assert current_update.contract_state_changes = &aggregated_update.contract_state_changes[
        aggregated_update.n_contract_state_changes
    ];
    assert current_update.contract_class_changes = &aggregated_update.contract_class_changes[
        aggregated_update.n_class_updates
    ];

    // Copy the messages.
    let final_carried_outputs = copy_l1l2_messages(
        aggregated_carried_outputs=aggregated.final_carried_outputs, current=&current
    );

    tempvar new_aggregated = new OsOutput(
        header=new OsOutputHeader(
            state_update_output=new CommitmentUpdate(
                initial_root=aggregated_header.state_update_output.initial_root,
                final_root=current_header.state_update_output.final_root,
            ),
            prev_block_number=aggregated_header.prev_block_number,
            new_block_number=current_header.new_block_number,
            prev_block_hash=aggregated_header.prev_block_hash,
            new_block_hash=current_header.new_block_hash,
            os_program_hash=aggregated_header.os_program_hash,
            starknet_os_config_hash=aggregated_header.starknet_os_config_hash,
            use_kzg_da=aggregated_header.use_kzg_da,
            full_output=aggregated_header.full_output,
            proof_facts_root_output_low=0,
            proof_facts_root_output_high=0,
            n_proof_facts_transactions=0,
        ),
        squashed_os_state_update=new SquashedOsStateUpdate(
            contract_state_changes=aggregated_update.contract_state_changes,
            n_contract_state_changes=(
                aggregated_update.n_contract_state_changes + current_update.n_contract_state_changes
            ),
            contract_class_changes=aggregated_update.contract_class_changes,
            n_class_updates=aggregated_update.n_class_updates + current_update.n_class_updates,
        ),
        initial_carried_outputs=aggregated.initial_carried_outputs,
        final_carried_outputs=final_carried_outputs,
    );

    return combine_blocks_inner(aggregated=new_aggregated, n=n - 1, os_outputs=&os_outputs[1]);
}

// Combines the per-block proof-fact fold results (see os/proof_fact_fold.cairo): folds
// the root entries of the blocks with contributing transactions into a single root
// entry, and returns its packed output digest together with the total number of
// contributing transactions. Blocks with no contributing transactions are skipped -
// they have no fold-tree node - and a single contributing block's root entry is carried
// unchanged. Returns zeros when no block contributed.
func combine_proof_facts_folds{range_check_ptr}(n: felt, os_outputs: OsOutput*) -> (
    n_proof_facts_transactions: felt, root_output_low: felt, root_output_high: felt
) {
    alloc_locals;
    let (local block_root_entries: felt*) = alloc();
    let (n_contributing_blocks, local n_proof_facts_transactions) = collect_block_root_entries(
        n=n, os_outputs=os_outputs, block_root_entries=block_root_entries
    );
    if (n_contributing_blocks == 0) {
        return (n_proof_facts_transactions=0, root_output_low=0, root_output_high=0);
    }
    let (root_entry) = fold_block_root_entries(
        n_entries=n_contributing_blocks, entries=block_root_entries
    );
    let (root_output_low, root_output_high) = pack_output_digest(entry=root_entry);
    return (
        n_proof_facts_transactions=n_proof_facts_transactions,
        root_output_low=root_output_low,
        root_output_high=root_output_high,
    );
}

// Writes the root entries of the blocks with contributing transactions, consecutively at
// `block_root_entries`, and returns the number of such blocks and the total number of
// contributing transactions. A block with no contributing transactions must carry a zero
// digest.
func collect_block_root_entries{range_check_ptr}(
    n: felt, os_outputs: OsOutput*, block_root_entries: felt*
) -> (n_contributing_blocks: felt, n_proof_facts_transactions: felt) {
    alloc_locals;
    if (n == 0) {
        return (n_contributing_blocks=0, n_proof_facts_transactions=0);
    }
    local header: OsOutputHeader* = os_outputs[0].header;
    local n_block_transactions = header.n_proof_facts_transactions;
    if (n_block_transactions == 0) {
        assert header.proof_facts_root_output_low = 0;
        assert header.proof_facts_root_output_high = 0;
        return collect_block_root_entries(
            n=n - 1, os_outputs=&os_outputs[1], block_root_entries=block_root_entries
        );
    }
    let (block_root_entry) = unpack_block_root_entry(
        low=header.proof_facts_root_output_low, high=header.proof_facts_root_output_high
    );
    memcpy(dst=block_root_entries, src=block_root_entry, len=FOLD_ENTRY_N_WORDS);
    let (n_rest_contributing_blocks, n_rest_transactions) = collect_block_root_entries(
        n=n - 1,
        os_outputs=&os_outputs[1],
        block_root_entries=block_root_entries + FOLD_ENTRY_N_WORDS,
    );
    return (
        n_contributing_blocks=n_rest_contributing_blocks + 1,
        n_proof_facts_transactions=n_rest_transactions + n_block_transactions,
    );
}
