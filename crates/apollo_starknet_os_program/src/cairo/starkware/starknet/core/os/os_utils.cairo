from starkware.cairo.common.bool import FALSE, TRUE
from starkware.cairo.common.cairo_builtins import EcOpBuiltin, PoseidonBuiltin
from starkware.cairo.common.dict import dict_update
from starkware.cairo.common.dict_access import DictAccess
from starkware.cairo.common.math_cmp import is_nn
from starkware.starknet.core.aggregator.combine_blocks import combine_blocks
from starkware.starknet.core.os.block_context import BlockContext, OsGlobalContext
from starkware.starknet.core.os.block_hash import get_block_hashes
from starkware.starknet.core.os.constants import (
    BLOCK_HASH_CONTRACT_ADDRESS,
    STORED_BLOCK_HASH_BUFFER,
)
from starkware.starknet.core.os.contract_class.blake_compiled_class_hash import (
    compiled_class_hash as blake_compiled_class_hash,
)
from starkware.starknet.core.os.contract_class.compiled_class import CompiledClassFact
from starkware.starknet.core.os.contract_class.poseidon_compiled_class_hash import (
    compiled_class_hash as poseidon_compiled_class_hash,
)
from starkware.starknet.core.os.output import OsOutput, OsOutputHeader, serialize_os_output
from starkware.starknet.core.os.state.commitment import CommitmentUpdate, StateEntry

// Performs pre-processing of the block: writes the block number to the block hash mapping and
// migrates contract classes to the v2 casm hash.
func pre_process_block{
    range_check_ptr,
    poseidon_ptr: PoseidonBuiltin*,
    contract_state_changes: DictAccess*,
    contract_class_changes: DictAccess*,
}(block_context: BlockContext*) {
    alloc_locals;

    write_block_number_to_block_hash_mapping(block_context=block_context);

    // Update the contract class changes according to the migration.
    local n_classes_to_migrate;
    // TODO(Meshi): Change to rust VM notion once all python tests only uses the rust VM.
    %{ ids.n_classes_to_migrate = len(block_input.class_hashes_to_migrate) %}
    migrate_classes_to_v2_casm_hash(n_classes=n_classes_to_migrate, block_context=block_context);
    return ();
}

// Writes the hash of the (current_block_number - buffer) block under its block number in the
// dedicated contract state, where buffer=STORED_BLOCK_HASH_BUFFER.
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
    %{ GetBlockHashMapping %}

    // Currently, the block hash mapping is not enforced by the OS.
    // TODO(Yoni, 1/1/2026): output this hash.
    local old_block_hash;
    %{ GetOldBlockNumberAndHash %}

    // Update mapping.
    assert state_entry.class_hash = 0;
    assert state_entry.nonce = 0;
    tempvar storage_ptr = state_entry.storage_ptr;
    assert [storage_ptr] = DictAccess(key=old_block_number, prev_value=0, new_value=old_block_hash);
    let storage_ptr = storage_ptr + DictAccess.SIZE;
    %{ WriteOldBlockToStorage %}

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
    // Sanity check: verify the guessed v2 hash.
    assert compiled_class_fact.hash = casm_hash_v2;
    // Update the casm hash from v1 to v2.
    dict_update{dict_ptr=contract_class_changes}(
        key=class_hash, prev_value=casm_hash_v1, new_value=casm_hash_v2
    );
    migrate_classes_to_v2_casm_hash(n_classes=n_classes - 1, block_context=block_context);
    return ();
}

// Returns the OS output header of the given block.
func get_block_os_output_header{poseidon_ptr: PoseidonBuiltin*}(
    block_context: BlockContext*,
    state_update_output: CommitmentUpdate*,
    os_global_context: OsGlobalContext*,
) -> OsOutputHeader* {
    // Calculate the block hash based on the block info and state root.
    // NOTE: both the previous block hash and previous state root are guessed, and the OS
    // does not verify their consistency (unlike the new hash and root).
    // The consumer of the OS output should verify both.
    let (prev_block_hash, new_block_hash) = get_block_hashes{poseidon_ptr=poseidon_ptr}(
        block_info=block_context.block_info_for_execute, state_root=state_update_output.final_root
    );

    // All blocks inside of a multi block should be off-chain and therefore
    // should not be compressed.
    tempvar os_output_header = new OsOutputHeader(
        state_update_output=state_update_output,
        prev_block_number=block_context.block_info_for_execute.block_number - 1,
        new_block_number=block_context.block_info_for_execute.block_number,
        prev_block_hash=prev_block_hash,
        new_block_hash=new_block_hash,
        os_program_hash=0,
        starknet_os_config_hash=os_global_context.starknet_os_config_hash,
        use_kzg_da=FALSE,
        full_output=TRUE,
    );
    return os_output_header;
}

// Processes OS outputs by combining blocks and serializing the result.
func process_os_output{
    output_ptr: felt*, range_check_ptr, ec_op_ptr: EcOpBuiltin*, poseidon_ptr: PoseidonBuiltin*
}(n_blocks: felt, os_outputs: OsOutput*, n_public_keys: felt, public_keys: felt*) {
    alloc_locals;
    // Guess whether to use KZG commitment scheme and whether to output the full state.
    // TODO(meshi): Once use_kzg_da field is used in the OS for the computation of fees and block
    //   hash, check that the `use_kzg_da` field is identical in all blocks in the multi-block.
    local use_kzg_da = nondet %{
        os_hints_config.use_kzg_da and (
            not os_hints_config.full_output
        )
    %};
    local full_output = nondet %{ os_hints_config.full_output %};

    // Verify that the guessed values are 0 or 1.
    assert use_kzg_da * use_kzg_da = use_kzg_da;
    assert full_output * full_output = full_output;

    let final_os_output = combine_blocks(
        n=n_blocks,
        os_outputs=os_outputs,
        os_program_hash=0,
        use_kzg_da=use_kzg_da,
        full_output=full_output,
    );

    // Serialize OS output.
    %{
        __serialize_data_availability_create_pages__ = True
        kzg_manager = global_hints.kzg_manager
    %}

    serialize_os_output(
        os_output=final_os_output,
        replace_keys_with_aliases=TRUE,
        n_public_keys=n_public_keys,
        public_keys=public_keys,
    );
    return ();
}
