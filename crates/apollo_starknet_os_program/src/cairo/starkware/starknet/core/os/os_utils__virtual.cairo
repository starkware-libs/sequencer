from starkware.cairo.common.bool import FALSE, TRUE
from starkware.cairo.common.builtin_poseidon.poseidon import poseidon_hash_many
from starkware.cairo.common.cairo_builtins import EcOpBuiltin, PoseidonBuiltin
from starkware.cairo.common.dict_access import DictAccess
from starkware.cairo.common.math import assert_not_zero
from starkware.starknet.core.os.block_context import BlockContext, OsGlobalContext
from starkware.starknet.core.os.block_hash import get_block_hashes
from starkware.starknet.core.os.output import MessageToL1Header, OsOutput, OsOutputHeader
from starkware.starknet.core.os.state.commitment import CommitmentUpdate
from starkware.starknet.core.os.virtual_os_output import (
    VIRTUAL_OS_OUTPUT_VERSION,
    VirtualOsOutputHeader,
)

// The (Sierra) class hashes that may not be executed in virtual OS mode.
// They are hardcoded into the program, so changing them changes the virtual OS program hash,
// which the Starknet OS must then accept (see `allowed_virtual_os_program_hashes`).
const BLOCKED_CLASS_HASH_0 = 0x26cd528392663c56cf80ce01006ae765595a267f838466573372a9f7b879d39;
const BLOCKED_CLASS_HASH_1 = 0x2bc06e58ffaa27a9c6ca32236265ac9886682df7edc84b01ac9f49da32f78a1;
const BLOCKED_CLASS_HASH_2 = 0x33f5e51a778015ec06e8263ab73e856b212e664cb41a88c28e5e3d6afbcd59a;
const BLOCKED_CLASS_HASH_3 = 0x34cdd05042a06e8c3f3c85d38676521dd7fac7a217d75b6f86cd80139408658;
const BLOCKED_CLASS_HASH_4 = 0x3bd627afcf1432a0c1d3c857d06c922e3aef00708fa008294131bfd99fa5898;
const BLOCKED_CLASS_HASH_5 = 0x4272b90fe4cb710711eaa3164fadc4ceefe5c0b7f735e7b2c86667168acdea2;
const BLOCKED_CLASS_HASH_6 = 0x57efda429008cd32e813fdbffb29489af59fad67f29cdf463475ee98ede6f1a;
const BLOCKED_CLASS_HASH_7 = 0x5ab171de8312f15966dc69e61515272012cc509aa029eb0637fca354b19b342;
const BLOCKED_CLASS_HASH_8 = 0x610671fae37007db26406df1a131cff3429929ccb61aac7a2a59887c200d80a;
const BLOCKED_CLASS_HASH_9 = 0x686937c657443b5b8fad37e5410424c76f9e7525000fd8a1b1d6f0337bb2e8;
const BLOCKED_CLASS_HASH_10 = 0x6def6cb177263c2a60d4fa956f1556fefce6278fb78db5ab26f7d15a73b9ec4;
const BLOCKED_CLASS_HASH_11 = 0x73c57a36974614728313132add861a4a92d491428a80625c59772e0db8e0869;
const BLOCKED_CLASS_HASH_12 = 0x8db496c7fc536d52d2606dde1b6b6dd803a6ae2ffe042158e56454c5671b78;
const BLOCKED_CLASS_HASH_13 = 0xed262147d9c3eb2e5e74976ba6a22a4b604e508d2eb9897683b0a556319909;
const BLOCKED_CLASS_HASH_14 = 0x3d909a8ecc20b65e76403c205815efa4e67ee943c530c56b45044c624fbb140;
const BLOCKED_CLASS_HASH_15 = 0x8b35c6c1a1757f431fd6aa46098cf79bf27f8eb0cec053fadac18ae7f923f0;
const BLOCKED_CLASS_HASH_16 = 0x1fbe90a5f781ca6da035891fff7de7bd7a1e7458aceb17f64799c849e68dd6;
const BLOCKED_CLASS_HASH_17 = 0x2502d3c88615ce06a6111a4eb1e3da35db654cbd86ae7196eee0f79c727aa70;
const BLOCKED_CLASS_HASH_18 = 0x47ab352acfc33b9898c8a8ee77849baf49a8f1bb34f6e0166fe17568bcf34bd;
const BLOCKED_CLASS_HASH_19 = 0x5a8c4d0e842562524a6708f4770f66275bcb9b4da2c3f248916bf2767f5be71;
const BLOCKED_CLASS_HASH_20 = 0x69e51760d41cc318562071a26094ef326916845e6684587a214fd11bfca5796;
const BLOCKED_CLASS_HASH_21 = 0x6a8b31afe4340cb5e9a03dc18b8cd192974717898a5274024a3ca8d5eb7dd4;
const BLOCKED_CLASS_HASH_22 = 0x73e566d30bf046ad0a409db5bca221f823d6f7bd1e85c3c649c8895e68e1230;
const BLOCKED_CLASS_HASH_23 = 0x370af5dee06f4239b0d55ac455c30bc6705665b06dc1f89dedefcff89ff134;
const BLOCKED_CLASS_HASH_24 = 0x4479fe755a479e26c13b90368d953a62b71fe55873d4ff21bf4417014837971;
const BLOCKED_CLASS_HASH_25 = 0x301d15309ca78d5ff6dcd8c7feb64defc8e7abdfb97c01972a12c64488364f0;
const BLOCKED_CLASS_HASH_26 = 0x3c6afc23aec0b0a7831ec68691a008914c8f458635d64ede6bffca4a08d84d6;
const BLOCKED_CLASS_HASH_27 = 0x74b0d7bcc0b6beefdd056b421ef6f52257581ce6760d6c8476c911cf8e0531;
const BLOCKED_CLASS_HASHES_LEN = 28;

// Hashes each L2-to-L1 message separately and writes the hash to the output.
func output_message_to_l1_hashes{output_ptr: felt*, poseidon_ptr: PoseidonBuiltin*}(
    messages_ptr_start: felt*, messages_ptr_end: felt*
) {
    if (messages_ptr_start == messages_ptr_end) {
        return ();
    }

    // Read the message header.
    let message_header = cast(messages_ptr_start, MessageToL1Header*);

    // Hash the message (header + payload).
    let message_size = MessageToL1Header.SIZE + message_header.payload_size;
    let (message_hash) = poseidon_hash_many(n=message_size, elements=messages_ptr_start);

    // Store the hash and advance output_ptr.
    assert output_ptr[0] = message_hash;
    let output_ptr = &output_ptr[1];

    return output_message_to_l1_hashes(
        messages_ptr_start=&messages_ptr_start[message_size], messages_ptr_end=messages_ptr_end
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
    // Calculate the previous block hash based on the block info and the **initial** state root.
    let (_prev_prev_block_hash, prev_block_hash) = get_block_hashes{poseidon_ptr=poseidon_ptr}(
        block_info=block_context.block_info_for_execute, state_root=state_update_output.initial_root
    );

    tempvar os_output_header = new OsOutputHeader(
        state_update_output=state_update_output,
        prev_block_number=block_context.block_info_for_execute.block_number,
        new_block_number=0,
        prev_block_hash=prev_block_hash,
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
    alloc_locals;
    assert n_public_keys = 0;

    // Part of the VIRTUAL_SNOS0 version contract. Changes must trigger a version bump.
    assert n_blocks = 1;
    let os_output = os_outputs[0];

    let squashed_os_state_update = os_output.squashed_os_state_update;
    assert_class_changes_not_blocked(
        n_class_changes=squashed_os_state_update.n_class_updates,
        class_changes=squashed_os_state_update.contract_class_changes,
    );

    let header = os_output.header;

    // Hash each message to L1 separately and write hashes to output.
    // We'll write hashes starting after the header (which we'll write later).
    let output_header_placeholder = cast(output_ptr, VirtualOsOutputHeader*);
    let output_ptr = output_ptr + VirtualOsOutputHeader.SIZE;
    let messages_to_l1_hashes_ptr_start: felt* = output_ptr;

    output_message_to_l1_hashes(
        messages_ptr_start=os_output.initial_carried_outputs.messages_to_l1,
        messages_ptr_end=os_output.final_carried_outputs.messages_to_l1,
    );

    // Calculate the number of messages from the pointer difference.
    let n_l2_to_l1_messages = output_ptr - messages_to_l1_hashes_ptr_start;

    // Create the virtual OS output header with count of messages.
    assert [output_header_placeholder] = VirtualOsOutputHeader(
        output_version=VIRTUAL_OS_OUTPUT_VERSION,
        base_block_number=header.prev_block_number,
        base_block_hash=header.prev_block_hash,
        starknet_os_config_hash=header.starknet_os_config_hash,
        n_l2_to_l1_messages=n_l2_to_l1_messages,
    );

    return ();
}

// Fails if any class in the squashed class changes is blocked.
// Every class the virtual OS executes is read from the class changes dict (see
// `execute_entry_point`), and the squashed dict has an entry for every accessed key, read-only
// accesses included, so this covers every executed class.
func assert_class_changes_not_blocked(n_class_changes: felt, class_changes: DictAccess*) {
    if (n_class_changes == 0) {
        return ();
    }
    assert_class_hash_not_blocked(class_hash=class_changes.key);
    return assert_class_changes_not_blocked(
        n_class_changes=n_class_changes - 1, class_changes=&class_changes[1]
    );
}

// Fails if `class_hash` is one of the blocked class hashes.
// The product of `class_hash - blocked_class_hash` over the blocked hashes is zero iff
// `class_hash` is one of them, as the field has no zero divisors.
func assert_class_hash_not_blocked(class_hash: felt) {
    static_assert BLOCKED_CLASS_HASHES_LEN == 28;
    with_attr error_message("Class hash is blocked in virtual OS mode") {
        assert_not_zero(
            (class_hash - BLOCKED_CLASS_HASH_0) * (class_hash - BLOCKED_CLASS_HASH_1) * (
                class_hash - BLOCKED_CLASS_HASH_2
            ) * (class_hash - BLOCKED_CLASS_HASH_3) * (class_hash - BLOCKED_CLASS_HASH_4) * (
                class_hash - BLOCKED_CLASS_HASH_5
            ) * (class_hash - BLOCKED_CLASS_HASH_6) * (class_hash - BLOCKED_CLASS_HASH_7) * (
                class_hash - BLOCKED_CLASS_HASH_8
            ) * (class_hash - BLOCKED_CLASS_HASH_9) * (class_hash - BLOCKED_CLASS_HASH_10) * (
                class_hash - BLOCKED_CLASS_HASH_11
            ) * (class_hash - BLOCKED_CLASS_HASH_12) * (class_hash - BLOCKED_CLASS_HASH_13) * (
                class_hash - BLOCKED_CLASS_HASH_14
            ) * (class_hash - BLOCKED_CLASS_HASH_15) * (class_hash - BLOCKED_CLASS_HASH_16) * (
                class_hash - BLOCKED_CLASS_HASH_17
            ) * (class_hash - BLOCKED_CLASS_HASH_18) * (class_hash - BLOCKED_CLASS_HASH_19) * (
                class_hash - BLOCKED_CLASS_HASH_20
            ) * (class_hash - BLOCKED_CLASS_HASH_21) * (class_hash - BLOCKED_CLASS_HASH_22) * (
                class_hash - BLOCKED_CLASS_HASH_23
            ) * (class_hash - BLOCKED_CLASS_HASH_24) * (class_hash - BLOCKED_CLASS_HASH_25) * (
                class_hash - BLOCKED_CLASS_HASH_26
            ) * (class_hash - BLOCKED_CLASS_HASH_27),
        );
    }
    return ();
}

// Returns whether aliases should be allocated for state updates.
// In virtual OS mode, aliases should not be allocated.
func should_allocate_aliases() -> felt {
    return FALSE;
}

// Returns a function pointer to execute_deprecated_syscalls.
// In virtual OS mode, deprecated syscalls are not supported, so we return 0.
func get_execute_deprecated_syscalls_ptr() -> (res: felt*) {
    return (res=cast(0, felt*));
}
