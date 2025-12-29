from starkware.cairo.common.bool import FALSE, TRUE
from starkware.cairo.common.cairo_builtins import EcOpBuiltin, PoseidonBuiltin
from starkware.cairo.common.dict_access import DictAccess
from starkware.cairo.common.segments import relocate_segment
from starkware.cairo.common.serialize import serialize_word
from starkware.starknet.core.os.block_context import BlockContext, OsGlobalContext, VirtualOsConfig
from starkware.starknet.core.os.block_hash import get_block_hashes
from starkware.starknet.core.os.output import OsOutput, OsOutputHeader
from starkware.starknet.core.os.state.commitment import CommitmentUpdate

const VIRTUAL_OS_OUTPUT_VERSION = 0;

// Does nothing for the virtual OS.
func pre_process_block{
    range_check_ptr,
    poseidon_ptr: PoseidonBuiltin*,
    contract_state_changes: DictAccess*,
    contract_class_changes: DictAccess*,
}(block_context: BlockContext*) {
    return ();
}

// Returns the OS output header for the virtual OS.
// Note that unlike the Starknet sequencer OS, the virtual OS expects the block info to be of
// the **previous** block.
func get_block_os_output_header{poseidon_ptr: PoseidonBuiltin*}(
    block_context: BlockContext*,
    state_update_output: CommitmentUpdate*,
    os_global_context: OsGlobalContext*,
) -> OsOutputHeader* {
    // Calculate the block hash based on the block info and the **initial** state root.
    let (_prev_block_hash, block_hash) = get_block_hashes{poseidon_ptr=poseidon_ptr}(
        block_info=block_context.block_info_for_execute, state_root=state_update_output.initial_root
    );

    tempvar os_output_header = new OsOutputHeader(
        state_update_output=state_update_output,
        prev_block_number=block_context.block_info_for_execute.block_number,
        new_block_number=0,
        prev_block_hash=block_hash,
        new_block_hash=0,
        os_program_hash=0,
        starknet_os_config_hash=os_global_context.starknet_os_config_hash,
        use_kzg_da=FALSE,
        full_output=TRUE,
    );
    return os_output_header;
}

// Processes OS outputs for the virtual OS.
// Outputs the virtual OS header and the messages to L1.
func process_os_output{
    output_ptr: felt*, range_check_ptr, ec_op_ptr: EcOpBuiltin*, poseidon_ptr: PoseidonBuiltin*
}(n_blocks: felt, os_outputs: OsOutput*, n_public_keys: felt, public_keys: felt*) {
    assert n_public_keys = 0;

    // Restrict the virtual OS to process a single block.
    assert n_blocks = 1;
    let os_output = os_outputs[0];

    // Serialize the header.
    let header = os_output.header;
    serialize_word(VIRTUAL_OS_OUTPUT_VERSION);
    serialize_word(header.prev_block_number);
    serialize_word(header.prev_block_hash);
    serialize_word(header.starknet_os_config_hash);
    // TODO(Yoni): output the authorized account address.

    // TODO(Yoni): output the hash of the messages instead.
    let messages_to_l1_segment_size = (
        os_output.final_carried_outputs.messages_to_l1 -
        os_output.initial_carried_outputs.messages_to_l1
    );
    serialize_word(messages_to_l1_segment_size);

    // Relocate 'messages_to_l1_segment' to the correct place in the output segment.
    relocate_segment(src_ptr=os_output.initial_carried_outputs.messages_to_l1, dest_ptr=output_ptr);
    let output_ptr = cast(os_output.final_carried_outputs.messages_to_l1, felt*);
    return ();
}

// Returns the virtual OS config.
func get_virtual_os_config() -> VirtualOsConfig {
    let virtual_os_config = VirtualOsConfig(enabled=TRUE);
    return virtual_os_config;
}
