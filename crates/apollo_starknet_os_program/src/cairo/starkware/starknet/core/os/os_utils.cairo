from starkware.cairo.common.bool import FALSE, TRUE
from starkware.cairo.common.dict import dict_update
from starkware.cairo.common.dict_access import DictAccess
from starkware.cairo.common.math_cmp import is_nn
from starkware.starknet.core.os.block_context import BlockContext
from starkware.starknet.core.os.constants import (
    BLOCK_HASH_CONTRACT_ADDRESS,
    STORED_BLOCK_HASH_BUFFER,
)
from starkware.starknet.core.os.state.commitment import StateEntry

func write_block_number_to_block_hash_mapping{range_check_ptr, contract_state_changes: DictAccess*}(
    block_context: BlockContext*
) {
    alloc_locals;
    tempvar old_block_number = block_context.block_info_for_execute.block_number -
        STORED_BLOCK_HASH_BUFFER;
    let is_old_block_number_non_negative = is_nn(old_block_number);
    if (is_old_block_number_non_negative == FALSE) {
        // Not enough blocks in the system - nothing to write.
        return ();
    }

    // Fetch the (block number -> block hash) mapping contract state.
    local state_entry: StateEntry*;
    %{
        ids.state_entry = __dict_manager.get_dict(ids.contract_state_changes)[
            ids.BLOCK_HASH_CONTRACT_ADDRESS
        ]
    %}

    // Currently, the block hash mapping is not enforced by the OS.
    // TODO(Yoni, 1/1/2026): output this hash.
    local old_block_hash;
    %{
        old_block_number_and_hash = block_input.old_block_number_and_hash
        assert (
            old_block_number_and_hash is not None
        ), f"Block number is probably < {ids.STORED_BLOCK_HASH_BUFFER}."
        (
            old_block_number, old_block_hash
        ) = old_block_number_and_hash
        assert old_block_number == ids.old_block_number,(
            "Inconsistent block number. "
            "The constant STORED_BLOCK_HASH_BUFFER is probably out of sync."
        )
        ids.old_block_hash = old_block_hash
    %}

    // Update mapping.
    assert state_entry.class_hash = 0;
    assert state_entry.nonce = 0;
    tempvar storage_ptr = state_entry.storage_ptr;
    assert [storage_ptr] = DictAccess(key=old_block_number, prev_value=0, new_value=old_block_hash);
    let storage_ptr = storage_ptr + DictAccess.SIZE;
    %{
        storage = execution_helper.storage_by_address[ids.BLOCK_HASH_CONTRACT_ADDRESS]
        storage.write(key=ids.old_block_number, value=ids.old_block_hash)
    %}

    // Update contract state.
    tempvar new_state_entry = new StateEntry(class_hash=0, storage_ptr=storage_ptr, nonce=0);
    dict_update{dict_ptr=contract_state_changes}(
        key=BLOCK_HASH_CONTRACT_ADDRESS,
        prev_value=cast(state_entry, felt),
        new_value=cast(new_state_entry, felt),
    );
    return ();
}

// Migrates contract classes from v1 (Poseidon-based CASM hash) to v2 (Blake-based CASM hash).
// The class hashes are guessed, and should at least cover the non-migrated classes that
// will be executed by the block.
// Hint arguments:
// block_input - The block input containing the class hashes to migrate.
// class_hashes_to_migrate_iterator - An iterator over the class hashes to migrate.
func migrate_classes_to_v2_casm_hash{
    poseidon_ptr: PoseidonBuiltin*, range_check_ptr, contract_class_changes: DictAccess*
}(n_classes: felt, block_context: BlockContext*) {
    alloc_locals;
    if (n_classes == 0) {
        return ();
    }
    // Guess the class hash and compiled class fact.
    local class_hash;
    local compiled_class_fact: CompiledClassFact*;
    %{ GetClassHashAndCompiledClassFact %}
    let compiled_class = compiled_class_fact.compiled_class;
    // Compute the full compiled class hash, both v1 and v2.
    // This hint enters a new scope that contains the bytecode segment structure of the class.
    %{ EnterScopeWithBytecodeSegmentStructure %}
    let (casm_hash_v1) = poseidon_compiled_class_hash(compiled_class, full_contract=TRUE);
    let (casm_hash_v2) = blake_compiled_class_hash(compiled_class, full_contract=TRUE);
    %{ vm_exit_scope() %}
    // Verify the guessed v2 hash.
    assert compiled_class_fact.hash = casm_hash_v2;
    // Update the casm hash from v1 to v2.
    dict_update{dict_ptr=contract_class_changes}(
        key=class_hash, prev_value=casm_hash_v1, new_value=casm_hash_v2
    );
    migrate_classes_to_v2_casm_hash(n_classes=n_classes - 1, block_context=block_context);
    return ();
}
