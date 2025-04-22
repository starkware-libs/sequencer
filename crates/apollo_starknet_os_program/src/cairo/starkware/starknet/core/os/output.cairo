from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.bool import FALSE
from starkware.cairo.common.cairo_builtins import PoseidonBuiltin
from starkware.cairo.common.dict import DictAccess
from starkware.cairo.common.registers import get_fp_and_pc
from starkware.cairo.common.segments import relocate_segment
from starkware.cairo.common.serialize import serialize_word
from starkware.starknet.core.os.data_availability.commitment import (
    OsKzgCommitmentInfo,
    Uint256,
    Uint384,
    compute_os_kzg_commitment_info,
)
from starkware.starknet.core.os.data_availability.compression import compress
from starkware.starknet.core.os.state.aliases import (
    replace_aliases_and_serialize_full_contract_state_diff,
)
from starkware.starknet.core.os.state.commitment import CommitmentUpdate
from starkware.starknet.core.os.state.output import (
    output_contract_class_da_changes,
    pack_contract_state_diff,
    serialize_full_contract_state_diff,
)
from starkware.starknet.core.os.state.state import SquashedOsStateUpdate

// Represents the output of the OS.
struct OsOutput {
    header: OsOutputHeader*,
    squashed_os_state_update: SquashedOsStateUpdate*,
    initial_carried_outputs: OsCarriedOutputs*,
    final_carried_outputs: OsCarriedOutputs*,
}

// The header of the OS output.
struct OsOutputHeader {
    state_update_output: CommitmentUpdate*,
    prev_block_number: felt,
    new_block_number: felt,
    prev_block_hash: felt,
    // Currently, the block hash is not enforced by the OS.
    new_block_hash: felt,
    // The hash of the OS program, if the aggregator was used. Zero if the OS was used directly.
    os_program_hash: felt,
    starknet_os_config_hash: felt,
    // Indicates whether to use KZG commitment scheme instead of adding the data-availability to
    // the transaction data.
    use_kzg_da: felt,
    // Indicates whether previous state values are included in the state update information.
    full_output: felt,
}

// An L2 to L1 message header, the message payload is concatenated to the end of the header.
struct MessageToL1Header {
    // The L2 address of the contract sending the message.
    from_address: felt,
    // The L1 address of the contract receiving the message.
    to_address: felt,
    payload_size: felt,
}

// An L1 to L2 message header, the message payload is concatenated to the end of the header.
struct MessageToL2Header {
    // The L1 address of the contract sending the message.
    from_address: felt,
    // The L2 address of the contract receiving the message.
    to_address: felt,
    nonce: felt,
    selector: felt,
    payload_size: felt,
}

// Holds all the information that StarkNet's OS needs to output.
struct OsCarriedOutputs {
    messages_to_l1: MessageToL1Header*,
    messages_to_l2: MessageToL2Header*,
}

func serialize_os_output{range_check_ptr, poseidon_ptr: PoseidonBuiltin*, output_ptr: felt*}(
    os_output: OsOutput*, replace_keys_with_aliases: felt
) {
    alloc_locals;

    local use_kzg_da = os_output.header.use_kzg_da;
    local full_output = os_output.header.full_output;
    let compress_state_updates = 1 - full_output;

    // Compute the data availability segment.
    local state_updates_start: felt*;
    let state_updates_ptr = state_updates_start;
    %{
        # `use_kzg_da` is used in a hint in `process_data_availability`.
        use_kzg_da = ids.use_kzg_da
        if use_kzg_da or ids.compress_state_updates:
            ids.state_updates_start = segments.add()
        else:
            # Assign a temporary segment, to be relocated into the output segment.
            ids.state_updates_start = segments.add_temp_segment()
    %}
    local squashed_os_state_update: SquashedOsStateUpdate* = os_output.squashed_os_state_update;
    with state_updates_ptr {
        // Output the contract state diff.
        output_contract_state(
            contract_state_changes_start=squashed_os_state_update.contract_state_changes,
            n_contract_state_changes=squashed_os_state_update.n_contract_state_changes,
            replace_keys_with_aliases=replace_keys_with_aliases,
            full_output=full_output,
        );

        // Output the contract class diff.
        output_contract_class_da_changes(
            update_ptr=squashed_os_state_update.contract_class_changes,
            n_updates=squashed_os_state_update.n_class_updates,
            full_output=full_output,
        );
    }

    serialize_output_header(os_output_header=os_output.header);

    let (local da_start, local da_end) = process_data_availability(
        state_updates_start=state_updates_start,
        state_updates_end=state_updates_ptr,
        compress_state_updates=compress_state_updates,
    );

    if (use_kzg_da != 0) {
        let os_kzg_commitment_info = compute_os_kzg_commitment_info(
            state_updates_start=da_start, state_updates_end=da_end
        );
        serialize_os_kzg_commitment_info(os_kzg_commitment_info=os_kzg_commitment_info);
        tempvar poseidon_ptr = poseidon_ptr;
        tempvar range_check_ptr = range_check_ptr;
    } else {
        // Align the stack with the `if` branch to avoid revoked references.
        tempvar output_ptr = output_ptr;
        tempvar poseidon_ptr = poseidon_ptr;
        tempvar range_check_ptr = range_check_ptr;
    }
    local range_check_ptr = range_check_ptr;
    local poseidon_ptr: PoseidonBuiltin* = poseidon_ptr;

    serialize_messages(
        initial_carried_outputs=os_output.initial_carried_outputs,
        final_carried_outputs=os_output.final_carried_outputs,
    );

    if (use_kzg_da == 0) {
        serialize_data_availability(da_start=da_start, da_end=da_end);
    }

    return ();
}

func os_carried_outputs_new(
    messages_to_l1: MessageToL1Header*, messages_to_l2: MessageToL2Header*
) -> (os_carried_outputs: OsCarriedOutputs*) {
    let (fp_val, pc_val) = get_fp_and_pc();
    static_assert OsCarriedOutputs.SIZE == Args.SIZE;
    return (os_carried_outputs=cast(fp_val - 2 - OsCarriedOutputs.SIZE, OsCarriedOutputs*));
}

// Serializes to output the constant-sized execution info needed for the L1 state update;
// for example, state roots and config hash.
func serialize_output_header{output_ptr: felt*}(os_output_header: OsOutputHeader*) {
    // Serialize program output.

    // Serialize roots.
    serialize_word(os_output_header.state_update_output.initial_root);
    serialize_word(os_output_header.state_update_output.final_root);
    serialize_word(os_output_header.prev_block_number);
    serialize_word(os_output_header.new_block_number);
    serialize_word(os_output_header.prev_block_hash);
    serialize_word(os_output_header.new_block_hash);
    serialize_word(os_output_header.os_program_hash);
    serialize_word(os_output_header.starknet_os_config_hash);
    serialize_word(os_output_header.use_kzg_da);
    serialize_word(os_output_header.full_output);

    return ();
}

// Serializes to output the L1<>L2 messages sent during the execution.
func serialize_messages{output_ptr: felt*}(
    initial_carried_outputs: OsCarriedOutputs*, final_carried_outputs: OsCarriedOutputs*
) {
    let messages_to_l1_segment_size = (
        final_carried_outputs.messages_to_l1 - initial_carried_outputs.messages_to_l1
    );
    serialize_word(messages_to_l1_segment_size);

    // Relocate 'messages_to_l1_segment' to the correct place in the output segment.
    relocate_segment(src_ptr=initial_carried_outputs.messages_to_l1, dest_ptr=output_ptr);
    let output_ptr = cast(final_carried_outputs.messages_to_l1, felt*);

    let messages_to_l2_segment_size = (
        final_carried_outputs.messages_to_l2 - initial_carried_outputs.messages_to_l2
    );
    serialize_word(messages_to_l2_segment_size);

    // Relocate 'messages_to_l2_segment' to the correct place in the output segment.
    relocate_segment(src_ptr=initial_carried_outputs.messages_to_l2, dest_ptr=output_ptr);
    let output_ptr = cast(final_carried_outputs.messages_to_l2, felt*);

    return ();
}

// Serializes OsKzgCommitmentInfo to output. Required for publishing data on L1 using KZG
// commitment; see `compute_os_kzg_commitment_info` documentation for more details.
func serialize_os_kzg_commitment_info{output_ptr: felt*}(
    os_kzg_commitment_info: OsKzgCommitmentInfo*
) {
    alloc_locals;
    local n_blobs = os_kzg_commitment_info.n_blobs;

    static_assert OsKzgCommitmentInfo.SIZE == 4;

    serialize_word(os_kzg_commitment_info.z);
    serialize_word(n_blobs);

    // Relocate 'kzg_commitments' to the correct place in the output segment.
    relocate_segment(src_ptr=os_kzg_commitment_info.kzg_commitments, dest_ptr=output_ptr);
    let output_ptr: felt* = &os_kzg_commitment_info.kzg_commitments[n_blobs];

    // Relocate 'evals' to the correct place in the output segment.
    relocate_segment(src_ptr=os_kzg_commitment_info.evals, dest_ptr=output_ptr);
    let output_ptr: felt* = &os_kzg_commitment_info.evals[n_blobs];

    return ();
}

// Returns the final data-availability to output.
func process_data_availability{range_check_ptr}(
    state_updates_start: felt*, state_updates_end: felt*, compress_state_updates: felt
) -> (da_start: felt*, da_end: felt*) {
    if (compress_state_updates == 0) {
        return (da_start=state_updates_start, da_end=state_updates_end);
    }

    alloc_locals;

    // Output a compression of the state updates.
    local compressed_start: felt*;
    %{
        if use_kzg_da:
            ids.compressed_start = segments.add()
        else:
            # Assign a temporary segment, to be relocated into the output segment.
            ids.compressed_start = segments.add_temp_segment()
    %}
    let compressed_dst = compressed_start;
    with compressed_dst {
        compress(data_start=state_updates_start, data_end=state_updates_end);
    }
    return (da_start=compressed_start, da_end=compressed_dst);
}

func serialize_data_availability{output_ptr: felt*}(da_start: felt*, da_end: felt*) {
    // Relocate data availability segment to the correct place in the output segment.
    relocate_segment(src_ptr=da_start, dest_ptr=output_ptr);
    let output_ptr = da_end;

    %{
        from starkware.python.math_utils import div_ceil

        if __serialize_data_availability_create_pages__:
            onchain_data_start = ids.da_start
            onchain_data_size = ids.output_ptr - onchain_data_start

            max_page_size = 3800
            n_pages = div_ceil(onchain_data_size, max_page_size)
            for i in range(n_pages):
                start_offset = i * max_page_size
                output_builtin.add_page(
                    page_id=1 + i,
                    page_start=onchain_data_start + start_offset,
                    page_size=min(onchain_data_size - start_offset, max_page_size),
                )
            # Set the tree structure to a root with two children:
            # * A leaf which represents the main part
            # * An inner node for the onchain data part (which contains n_pages children).
            #
            # This is encoded using the following sequence:
            output_builtin.add_attribute('gps_fact_topology', [
                # Push 1 + n_pages pages (all of the pages).
                1 + n_pages,
                # Create a parent node for the last n_pages.
                n_pages,
                # Don't push additional pages.
                0,
                # Take the first page (the main part) and the node that was created (onchain data)
                # and use them to construct the root of the fact tree.
                2,
            ])
    %}

    return ();
}

// Serializes the contract state diff into `state_updates_ptr`, to make this data available
// on-chain.
// - If `replace_keys_with_aliases` is on, replaces addresses and storage keys with their aliases.
// - If `full_output` is off, writes a shortened version of the diff; in particular, packs contract
//   headers and drops the previous values of storage cells (see `pack_contract_state_diff`).
//
// Assumption: The dictionary `contract_state_changes_start` is squashed.
func output_contract_state{range_check_ptr, state_updates_ptr: felt*}(
    contract_state_changes_start: DictAccess*,
    n_contract_state_changes: felt,
    replace_keys_with_aliases: felt,
    full_output: felt,
) {
    alloc_locals;
    if (full_output != FALSE) {
        // The full state diff can be written directly to `state_updates_ptr`.
        serialize_contract_state_diff_conditional{res=state_updates_ptr}(
            n_contracts=n_contract_state_changes,
            contract_state_changes=contract_state_changes_start,
            replace_keys_with_aliases=replace_keys_with_aliases,
        );
        return ();
    }

    // Serialize the full contract state diff into a new segment.
    let contract_state_diff_start: felt* = alloc();
    let contract_state_diff = contract_state_diff_start;
    serialize_contract_state_diff_conditional{res=contract_state_diff}(
        n_contracts=n_contract_state_changes,
        contract_state_changes=contract_state_changes_start,
        replace_keys_with_aliases=replace_keys_with_aliases,
    );
    // Write the packed diff into `state_updates_ptr`.
    pack_contract_state_diff{res=state_updates_ptr}(contract_state_diff=contract_state_diff_start);
    return ();
}

// Serializes the full contract state diff into `res`.
// If `replace_keys_with_aliases` is True, replaces contract addresses and storage keys
// with their aliases.
func serialize_contract_state_diff_conditional{range_check_ptr, res: felt*}(
    n_contracts: felt, contract_state_changes: DictAccess*, replace_keys_with_aliases: felt
) {
    if (replace_keys_with_aliases != FALSE) {
        return replace_aliases_and_serialize_full_contract_state_diff(
            n_contracts=n_contracts, contract_state_changes=contract_state_changes
        );
    }
    // The contract state changes is already represented with aliases instead of keys -
    // this flow is relevant only to the aggregator, where the block state diffs are loaded after
    // the alias replacement.
    return serialize_full_contract_state_diff(
        n_contracts=n_contracts, contract_state_changes=contract_state_changes
    );
}
