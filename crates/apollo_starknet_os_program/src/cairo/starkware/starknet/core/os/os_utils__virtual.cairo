from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.bool import FALSE, TRUE
from starkware.cairo.common.builtin_poseidon.poseidon import poseidon_hash_many
from starkware.cairo.common.cairo_builtins import EcOpBuiltin, PoseidonBuiltin
from starkware.cairo.common.dict_access import DictAccess
from starkware.cairo.common.memcpy import memcpy
from starkware.starknet.core.os.block_context import BlockContext, OsGlobalContext, VirtualOsConfig
from starkware.starknet.core.os.block_hash import get_block_hashes
from starkware.starknet.core.os.output import MessageToL1Header, OsOutput, OsOutputHeader
from starkware.starknet.core.os.state.commitment import CommitmentUpdate
from starkware.starknet.core.os.virtual_os_output import VirtualOsOutputHeader

const VIRTUAL_OS_OUTPUT_VERSION = 0;

// Recursively hashes each L2-to-L1 message separately and writes them.
func hash_messages_to_l1_recursive{output_ptr: felt*, poseidon_ptr: PoseidonBuiltin*}(
    messages_ptr_start: MessageToL1Header*, messages_ptr_end: MessageToL1Header*
) {
    alloc_locals;

    // Base case: if we've processed all messages, return.
    if (messages_ptr_start == messages_ptr_end) {
        return ();
    }

    // Read the message header.
    let message_header = [messages_ptr_start];
    let payload_size = message_header.payload_size;

    // Hash the message (header + payload).
    // The message consists of: from_address, to_address, payload_size, ...payload.
    let message_size = MessageToL1Header.SIZE + payload_size;
    let (message_hash) = poseidon_hash_many(
        n=message_size, elements=cast(messages_ptr_start, felt*)
    );

    // Store the hash and advance output_ptr.
    assert [output_ptr] = message_hash;
    let output_ptr = output_ptr + 1;

    // Move to the next message.
    let next_message_ptr = cast(
        messages_ptr_start + MessageToL1Header.SIZE + payload_size, MessageToL1Header*
    );

    // Recursively process the remaining messages.
    return hash_messages_to_l1_recursive(
        messages_ptr_start=next_message_ptr, messages_ptr_end=messages_ptr_end
    );
}

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
}(
    n_blocks: felt,
    os_outputs: OsOutput*,
    n_public_keys: felt,
    public_keys: felt*,
    os_global_context: OsGlobalContext*,
) {
    alloc_locals;
    assert n_public_keys = 0;

    // Restrict the virtual OS to process a single block.
    assert n_blocks = 1;
    let os_output = os_outputs[0];

    // Serialize the header using memcpy to enforce struct field order.
    let header = os_output.header;

    // Hash each message to L1 separately and write hashes to output.
    // We'll write hashes starting after the header (which we'll write later).
    let output_header_placeholder: felt* = output_ptr;
    let output_ptr = output_ptr + VirtualOsOutputHeader.SIZE;
    let messages_to_l1_hashes_ptr_start: felt* = output_ptr;

    hash_messages_to_l1_recursive(
        messages_ptr_start=os_output.initial_carried_outputs.messages_to_l1,
        messages_ptr_end=os_output.final_carried_outputs.messages_to_l1,
    );

    // Calculate the number of messages from the pointer difference.
    let n_messages_to_l1 = output_ptr - messages_to_l1_hashes_ptr_start;

    // Create the virtual OS output header with count of messages.
    tempvar virtual_os_output_header = new VirtualOsOutputHeader(
        version=VIRTUAL_OS_OUTPUT_VERSION,
        base_block_number=header.prev_block_number,
        base_block_hash=header.prev_block_hash,
        starknet_os_config_hash=os_global_context.starknet_os_config_hash,
        authorized_account_address=os_global_context.virtual_os_config.authorized_account_address,
        n_messages_to_l1=n_messages_to_l1,
    );

    // Copy the header to the output.
    memcpy(
        dst=output_header_placeholder, src=virtual_os_output_header, len=VirtualOsOutputHeader.SIZE
    );

    return ();
}

// Returns the virtual OS config.
func get_virtual_os_config() -> VirtualOsConfig* {
    let (virtual_os_config: VirtualOsConfig*) = alloc();
    static_assert VirtualOsConfig.SIZE == 2;
    assert virtual_os_config.enabled = TRUE;
    // The authorized account address will be set during transaction execution.
    return virtual_os_config;
}

// Returns a function pointer to execute_deprecated_syscalls.
// In virtual OS mode, deprecated syscalls are not supported, so we return 0.
func get_execute_deprecated_syscalls_ptr() -> (res: felt*) {
    return (res=cast(0, felt*));
}
