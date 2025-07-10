use blockifier::state::state_api::StateReader;
use indoc::indoc;
#[cfg(any(test, feature = "testing"))]
use serde::Serialize;
#[cfg(any(test, feature = "testing"))]
use strum::IntoEnumIterator;

use crate::hint_processor::aggregator_hint_processor::AggregatorHintProcessor;
use crate::hint_processor::common_hint_processor::CommonHintProcessor;
use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::{OsHintError, OsHintExtensionResult, OsHintResult};
use crate::hints::hint_implementation::aggregator::{
    allocate_segments_for_messages,
    disable_da_page_creation,
    get_aggregator_output,
    get_full_output_from_input,
    get_os_output_for_inner_blocks,
    get_use_kzg_da_from_input,
    set_state_update_pointers_to_none,
    write_da_segment,
};
use crate::hints::hint_implementation::block_context::{
    block_number,
    block_timestamp,
    chain_id,
    fee_token_address,
    get_block_mapping,
    sequencer_address,
    write_use_kzg_da_to_memory,
};
use crate::hints::hint_implementation::bls_field::compute_ids_low;
use crate::hints::hint_implementation::builtins::{
    select_builtin,
    selected_builtins,
    update_builtin_ptrs,
};
use crate::hints::hint_implementation::cairo1_revert::{
    generate_dummy_os_output_segment,
    prepare_state_entry_for_revert,
    read_storage_key_for_revert,
    write_storage_key_for_revert,
};
use crate::hints::hint_implementation::compiled_class::implementation::{
    assert_end_of_bytecode_segments,
    assign_bytecode_segments,
    bytecode_segment_structure,
    delete_memory_data,
    is_leaf,
    iter_current_segment_info,
    load_class,
    load_class_inner,
    set_ap_to_segment_hash,
    validate_compiled_class_facts_post_execution,
};
use crate::hints::hint_implementation::deprecated_compiled_class::implementation::{
    load_deprecated_class,
    load_deprecated_class_facts,
    load_deprecated_class_inner,
};
use crate::hints::hint_implementation::execute_syscalls::is_block_number_in_block_hash_buffer;
use crate::hints::hint_implementation::execute_transactions::implementation::{
    fill_holes_in_rc96_segment,
    log_remaining_txs,
    os_input_transactions,
    segments_add,
    segments_add_temp,
    set_ap_to_actual_fee,
    set_component_hashes,
    set_sha256_segment_in_syscall_handler,
    sha2_finalize,
    skip_tx,
    start_tx,
};
use crate::hints::hint_implementation::execution::implementation::{
    assert_transaction_hash,
    cache_contract_storage_request_key,
    cache_contract_storage_syscall_request_address,
    check_execution,
    check_is_deprecated,
    check_new_deploy_response,
    check_new_syscall_response,
    check_syscall_response,
    contract_address,
    declare_tx_fields,
    end_tx,
    enter_call,
    enter_scope_deprecated_syscall_handler,
    enter_scope_syscall_handler,
    enter_syscall_scopes,
    exit_call,
    exit_tx,
    fetch_result,
    gen_signature_arg,
    get_block_hash_contract_address_state_entry_and_set_new_state_entry,
    get_contract_address_state_entry,
    get_old_block_number_and_hash,
    initial_ge_required_gas,
    is_deprecated,
    is_remaining_gas_lt_initial_budget,
    is_reverted,
    load_next_tx,
    load_resource_bounds,
    prepare_constructor_execution,
    set_ap_to_tx_nonce,
    set_fp_plus_4_to_tx_nonce,
    set_state_entry_to_account_contract_address,
    tx_account_deployment_data,
    tx_account_deployment_data_len,
    tx_calldata,
    tx_calldata_len,
    tx_entry_point_selector,
    tx_fee_data_availability_mode,
    tx_nonce_data_availability_mode,
    tx_paymaster_data,
    tx_paymaster_data_len,
    tx_tip,
    tx_version,
    write_old_block_to_storage,
    write_syscall_result,
    write_syscall_result_deprecated,
};
use crate::hints::hint_implementation::find_element::search_sorted_optimistic;
use crate::hints::hint_implementation::kzg::implementation::{
    store_da_segment,
    write_split_result,
};
use crate::hints::hint_implementation::math::log2_ceil;
use crate::hints::hint_implementation::os::{
    configure_kzg_manager,
    create_block_additional_hints,
    get_n_blocks,
    init_state_update_pointer,
    initialize_class_hashes,
    initialize_state_changes,
    log_remaining_blocks,
    set_ap_to_new_block_hash,
    set_ap_to_prev_block_hash,
    starknet_os_input,
    write_full_output_to_memory,
};
use crate::hints::hint_implementation::os_logger::{
    log_enter_syscall,
    os_logger_enter_syscall_prepare_exit_syscall,
    os_logger_exit_syscall,
};
use crate::hints::hint_implementation::output::{
    set_compressed_start,
    set_n_updates_small,
    set_state_updates_start,
    set_tree_structure,
};
use crate::hints::hint_implementation::patricia::implementation::{
    assert_case_is_right,
    build_descent_map,
    decode_node,
    enter_scope_descend_edge,
    enter_scope_left_child,
    enter_scope_new_node,
    enter_scope_next_node_bit_0,
    enter_scope_next_node_bit_1,
    enter_scope_node,
    enter_scope_right_child,
    height_is_zero_or_len_node_preimage_is_two,
    is_case_right,
    load_bottom,
    load_edge,
    prepare_preimage_validation_non_deterministic_hashes,
    set_ap_to_descend,
    set_bit,
    set_siblings,
    split_descend,
    write_case_not_left_to_ap,
};
use crate::hints::hint_implementation::resources::{
    debug_expected_initial_gas,
    is_sierra_gas_mode,
    remaining_gas_gt_max,
};
use crate::hints::hint_implementation::secp::{is_on_curve, read_ec_point_from_address};
use crate::hints::hint_implementation::state::{
    compute_commitments_on_finalized_state_with_aliases,
    guess_classes_ptr,
    guess_state_ptr,
    set_preimage_for_class_commitments,
    set_preimage_for_current_commitment_info,
    set_preimage_for_state_commitments,
    update_classes_ptr,
    update_state_ptr,
};
use crate::hints::hint_implementation::stateful_compression::implementation::{
    assert_key_big_enough_for_alias,
    contract_address_le_max_for_compression,
    enter_scope_with_aliases,
    guess_aliases_contract_storage_ptr,
    guess_contract_addr_storage_ptr,
    initialize_alias_counter,
    key_lt_min_alias_alloc_value,
    read_alias_counter,
    read_alias_from_key,
    update_alias_counter,
    update_aliases_contract_to_storage_ptr,
    update_contract_addr_to_storage_ptr,
    write_next_alias_from_key,
};
use crate::hints::hint_implementation::stateless_compression::implementation::{
    compression_hint,
    dictionary_from_bucket,
    get_prev_offset,
    set_decompressed_dst,
};
use crate::hints::hint_implementation::syscalls::{
    call_contract,
    delegate_call,
    delegate_l1_handler,
    deploy,
    emit_event,
    get_block_number,
    get_block_timestamp,
    get_caller_address,
    get_contract_address,
    get_sequencer_address,
    get_tx_info,
    get_tx_signature,
    library_call,
    library_call_l1_handler,
    replace_class,
    send_message_to_l1,
    set_syscall_ptr,
    storage_read,
    storage_write,
};
use crate::hints::types::{HintArgs, HintEnum};
use crate::{
    define_common_hint_enum,
    define_hint_enum,
    define_hint_extension_enum,
    define_stateless_hint_enum,
};

#[cfg(test)]
#[path = "enum_definition_test.rs"]
pub mod test;

#[cfg(any(test, feature = "testing"))]
pub(crate) const TEST_HINT_PREFIX: &str = "# TEST HINT";

macro_rules! all_hints_enum {
    ($($inner_enum:ident),+) => {
        #[cfg_attr(any(test, feature = "testing"),derive(Serialize, strum_macros::EnumIter))]
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
        pub enum AllHints {
            $($inner_enum($inner_enum),)+
            #[cfg(any(test, feature = "testing"))]
            TestHint
        }

        #[cfg(any(test, feature = "testing"))]
        impl AllHints {
            pub fn all_iter() -> impl Iterator<Item = AllHints> {
                Self::iter().flat_map(|default_inner_variant| match default_inner_variant {
                    $(
                        Self::$inner_enum(_) => {
                            $inner_enum::iter().map(Self::from).collect::<Vec<Self>>()
                        }
                    )+
                    #[cfg(any(test, feature = "testing"))]
                    // Ignore the test hint in the iterator.
                    Self::TestHint => vec![],
                })
            }
        }

        impl HintEnum for AllHints {
            fn from_str(hint_str: &str) -> Result<Self, OsHintError> {
                #[cfg(any(test, feature = "testing"))]
                {
                    if hint_str.to_string().trim().starts_with(TEST_HINT_PREFIX) {
                        return Ok(Self::TestHint);
                    }
                }
                $(
                    if let Ok(hint) = $inner_enum::from_str(hint_str) {
                        return Ok(hint.into())
                    }
                )+
                Err(OsHintError::UnknownHint(hint_str.to_string()))
            }

            fn to_str(&self) -> &'static str {
                match self {
                    $(Self::$inner_enum(hint) => hint.to_str(),)+
                    #[cfg(any(test, feature = "testing"))]
                    Self::TestHint => {
                        panic!("Cannot convert TestHint to string; actual string unknown.")
                    }
                }
            }
        }
    }
}

all_hints_enum!(
    StatelessHint,
    CommonHint,
    DeprecatedSyscallHint,
    OsHint,
    HintExtension,
    AggregatorHint
);

define_hint_enum!(
    DeprecatedSyscallHint,
    SnosHintProcessor<'_, S>,
    S,
    StateReader,
    (
        CallContract,
        call_contract,
        "syscall_handler.call_contract(segments=segments, syscall_ptr=ids.syscall_ptr)"
    ),
    (
        DelegateCall,
        delegate_call,
        "syscall_handler.delegate_call(segments=segments, syscall_ptr=ids.syscall_ptr)"
    ),
    (
        DelegateL1Handler,
        delegate_l1_handler,
        "syscall_handler.delegate_l1_handler(segments=segments, syscall_ptr=ids.syscall_ptr)"
    ),
    (Deploy, deploy, "syscall_handler.deploy(segments=segments, syscall_ptr=ids.syscall_ptr)"),
    (
        EmitEvent,
        emit_event,
        "syscall_handler.emit_event(segments=segments, syscall_ptr=ids.syscall_ptr)"
    ),
    (
        GetBlockNumber,
        get_block_number,
        "syscall_handler.get_block_number(segments=segments, syscall_ptr=ids.syscall_ptr)"
    ),
    (
        GetBlockTimestamp,
        get_block_timestamp,
        "syscall_handler.get_block_timestamp(segments=segments, syscall_ptr=ids.syscall_ptr)"
    ),
    (
        GetCallerAddress,
        get_caller_address,
        "syscall_handler.get_caller_address(segments=segments, syscall_ptr=ids.syscall_ptr)"
    ),
    (
        GetContractAddress,
        get_contract_address,
        "syscall_handler.get_contract_address(segments=segments, syscall_ptr=ids.syscall_ptr)"
    ),
    (
        GetSequencerAddress,
        get_sequencer_address,
        "syscall_handler.get_sequencer_address(segments=segments, syscall_ptr=ids.syscall_ptr)"
    ),
    (
        GetTxInfo,
        get_tx_info,
        "syscall_handler.get_tx_info(segments=segments, syscall_ptr=ids.syscall_ptr)"
    ),
    (
        GetTxSignature,
        get_tx_signature,
        "syscall_handler.get_tx_signature(segments=segments, syscall_ptr=ids.syscall_ptr)"
    ),
    (
        LibraryCall,
        library_call,
        "syscall_handler.library_call(segments=segments, syscall_ptr=ids.syscall_ptr)"
    ),
    (
        LibraryCallL1Handler,
        library_call_l1_handler,
        "syscall_handler.library_call_l1_handler(segments=segments, syscall_ptr=ids.syscall_ptr)"
    ),
    (
        ReplaceClass,
        replace_class,
        "syscall_handler.replace_class(segments=segments, syscall_ptr=ids.syscall_ptr)"
    ),
    (
        SendMessageToL1,
        send_message_to_l1,
        "syscall_handler.send_message_to_l1(segments=segments, syscall_ptr=ids.syscall_ptr)"
    ),
    (
        StorageRead,
        storage_read,
        "syscall_handler.storage_read(segments=segments, syscall_ptr=ids.syscall_ptr)"
    ),
    (
        StorageWrite,
        storage_write,
        "syscall_handler.storage_write(segments=segments, syscall_ptr=ids.syscall_ptr)"
    ),
);

define_stateless_hint_enum!(
    StatelessHint,
    (
        IsBlockNumberInBlockHashBuffer,
        is_block_number_in_block_hash_buffer,
        // CHANGED: whitespaces.
        r#"memory[ap] = to_felt_or_relocatable(ids.request_block_number > \
           ids.current_block_number - ids.STORED_BLOCK_HASH_BUFFER)"#
    ),
    (
        GetBlockMapping,
        get_block_mapping,
        indoc! {r#"
    ids.state_entry = __dict_manager.get_dict(ids.contract_state_changes)[
        ids.BLOCK_HASH_CONTRACT_ADDRESS
    ]"#}
    ),
    (
        IsLeaf,
        is_leaf,
        indoc! {r#"
    from starkware.starknet.core.os.contract_class.compiled_class_hash_objects import (
        BytecodeLeaf,
    )
    ids.is_leaf = 1 if isinstance(bytecode_segment_structure, BytecodeLeaf) else 0"#}
    ),
    (
        SelectedBuiltins,
        selected_builtins,
        "vm_enter_scope({'n_selected_builtins': ids.n_selected_builtins})"
    ),
    (
        SelectBuiltin,
        select_builtin,
        indoc! {r##"
    # A builtin should be selected iff its encoding appears in the selected encodings list
    # and the list wasn't exhausted.
    # Note that testing inclusion by a single comparison is possible since the lists are sorted.
    ids.select_builtin = int(
      n_selected_builtins > 0 and memory[ids.selected_encodings] == memory[ids.all_encodings])
    if ids.select_builtin:
      n_selected_builtins = n_selected_builtins - 1"##
        }
    ),
    (
        PrepareStateEntryForRevert,
        prepare_state_entry_for_revert,
        indoc! {r#"# Fetch a state_entry in this hint and validate it in the update that comes next.
        ids.state_entry = __dict_manager.get_dict(ids.contract_state_changes)[ids.contract_address]

        # Fetch the relevant storage.
        storage = execution_helper.storage_by_address[ids.contract_address]"#}
    ),
    (
        GenerateDummyOsOutputSegment,
        generate_dummy_os_output_segment,
        "memory[ap] = to_felt_or_relocatable(segments.gen_arg([[], 0]))"
    ),
    (
        AssignBytecodeSegments,
        assign_bytecode_segments,
        indoc! {r#"
            bytecode_segments = iter(bytecode_segment_structure.segments)"#
        }
    ),
    (
        AssertEndOfBytecodeSegments,
        assert_end_of_bytecode_segments,
        indoc! {r#"
            assert next(bytecode_segments, None) is None"#
        }
    ),
    (
        DeleteMemoryData,
        delete_memory_data,
        indoc! {r#"
            # Sanity check.
            assert not is_accessed(ids.data_ptr), "The segment is skipped but was accessed."
            del memory.data[ids.data_ptr]"#
        }
    ),
    (
        IterCurrentSegmentInfo,
        iter_current_segment_info,
        indoc! {r#"
    current_segment_info = next(bytecode_segments)

    is_used = is_segment_used_callback(ids.data_ptr, current_segment_info.segment_length)
    ids.is_segment_used = 1 if is_used else 0

    is_used_leaf = is_used and isinstance(current_segment_info.inner_structure, BytecodeLeaf)
    ids.is_used_leaf = 1 if is_used_leaf else 0

    ids.segment_length = current_segment_info.segment_length
    vm_enter_scope(new_scope_locals={
        "bytecode_segment_structure": current_segment_info.inner_structure,
        "is_segment_used_callback": is_segment_used_callback
    })"#
        }
    ),
    (
        SetApToSegmentHash,
        set_ap_to_segment_hash,
        indoc! {r#"
            memory[ap] = to_felt_or_relocatable(bytecode_segment_structure.hash())"#
        }
    ),
    (
        EnterScopeWithAliases,
        enter_scope_with_aliases,
        indoc! {r#"from starkware.starknet.definitions.constants import ALIAS_CONTRACT_ADDRESS

# This hint shouldn't be whitelisted.
vm_enter_scope(dict(
    state_update_pointers=state_update_pointers,
    aliases=execution_helper.storage_by_address[ALIAS_CONTRACT_ADDRESS],
    execution_helper=execution_helper,
    __dict_manager=__dict_manager,
    block_input=block_input,
))"#}
    ),
    (
        KeyLtMinAliasAllocValue,
        key_lt_min_alias_alloc_value,
        "memory[ap] = to_felt_or_relocatable(ids.key < ids.MIN_VALUE_FOR_ALIAS_ALLOC)"
    ),
    (
        AssertKeyBigEnoughForAlias,
        assert_key_big_enough_for_alias,
        r#"assert ids.key >= ids.MIN_VALUE_FOR_ALIAS_ALLOC, f"Key {ids.key} is too small.""#
    ),
    (
        ContractAddressLeMaxForCompression,
        contract_address_le_max_for_compression,
        "memory[ap] = to_felt_or_relocatable(ids.contract_address <= \
         ids.MAX_NON_COMPRESSED_CONTRACT_ADDRESS)"
    ),
    (
        ComputeCommitmentsOnFinalizedStateWithAliases,
        compute_commitments_on_finalized_state_with_aliases,
        "commitment_info_by_address=execution_helper.compute_storage_commitments()"
    ),
    (
        DictionaryFromBucket,
        dictionary_from_bucket,
        indoc! {
            r#"initial_dict = {bucket_index: 0 for bucket_index in range(ids.TOTAL_N_BUCKETS)}"#
        }
    ),
    (
        GetPrevOffset,
        get_prev_offset,
        indoc! {r#"dict_tracker = __dict_manager.get_tracker(ids.dict_ptr)
            ids.prev_offset = dict_tracker.data[ids.bucket_index]"#
        }
    ),
    (
        CompressionHint,
        compression_hint,
        indoc! {r#"from starkware.starknet.core.os.data_availability.compression import compress
    data = memory.get_range_as_ints(addr=ids.data_start, size=ids.data_end - ids.data_start)
    segments.write_arg(ids.compressed_dst, compress(data))"#}
    ),
    (
        SetDecompressedDst,
        set_decompressed_dst,
        indoc! {r#"memory[ids.decompressed_dst] = ids.packed_felt % ids.elm_bound"#
        }
    ),
    (
        SegmentsAddTemp,
        segments_add_temp,
        indoc! {r#"memory[fp + 7] = to_felt_or_relocatable(segments.add_temp_segment())"#
        }
    ),
    (
        SegmentsAdd,
        segments_add,
        indoc! {r#"memory[ap] = to_felt_or_relocatable(segments.add())"#
        }
    ),
    (
        LogRemainingTxs,
        log_remaining_txs,
        indoc! {r#"print(f"execute_transactions_inner: {ids.n_txs} transactions remaining.")"#}
    ),
    (
        FillHolesInRc96Segment,
        fill_holes_in_rc96_segment,
        indoc! {r#"
rc96_ptr = ids.range_check96_ptr
segment_size = rc96_ptr.offset
base = rc96_ptr - segment_size

for i in range(segment_size):
    memory.setdefault(base + i, 0)"#}
    ),
    (
        Sha2Finalize,
        sha2_finalize,
        indoc! {r#"# Add dummy pairs of input and output.
from starkware.cairo.common.cairo_sha256.sha256_utils import (
    IV,
    compute_message_schedule,
    sha2_compress_function,
)

number_of_missing_blocks = (-ids.n) % ids.BATCH_SIZE
assert 0 <= number_of_missing_blocks < 20
_sha256_input_chunk_size_felts = ids.SHA256_INPUT_CHUNK_SIZE_FELTS
assert 0 <= _sha256_input_chunk_size_felts < 100

message = [0] * _sha256_input_chunk_size_felts
w = compute_message_schedule(message)
output = sha2_compress_function(IV, w)
padding = (message + IV + output) * number_of_missing_blocks
segments.write_arg(ids.sha256_ptr_end, padding)"#}
    ),
    (
        EnterScopeDeprecatedSyscallHandler,
        enter_scope_deprecated_syscall_handler,
        "vm_enter_scope({'syscall_handler': deprecated_syscall_handler})"
    ),
    (
        EnterScopeSyscallHandler,
        enter_scope_syscall_handler,
        "vm_enter_scope({'syscall_handler': syscall_handler})"
    ),
    (
        GetContractAddressStateEntry,
        get_contract_address_state_entry,
        indoc! {r#"
    # Fetch a state_entry in this hint and validate it in the update at the end
    # of this function.
    ids.state_entry = __dict_manager.get_dict(ids.contract_state_changes)[ids.contract_address]"#
        }
    ),
    (
        GetBlockHashContractAddressStateEntryAndSetNewStateEntry,
        get_block_hash_contract_address_state_entry_and_set_new_state_entry,
        indoc! {r#"
	# Fetch a state_entry in this hint. Validate it in the update that comes next.
	ids.state_entry = __dict_manager.get_dict(ids.contract_state_changes)[
	    ids.BLOCK_HASH_CONTRACT_ADDRESS]"#
        }
    ),
    (
        GetContractAddressStateEntryAndSetNewStateEntry,
        get_contract_address_state_entry,
        indoc! {r#"
    # Fetch a state_entry in this hint and validate it in the update that comes next.
    ids.state_entry = __dict_manager.get_dict(ids.contract_state_changes)[ids.contract_address]"#
        }
    ),
    (
        GetContractAddressStateEntryAndSetNewStateEntry2,
        get_contract_address_state_entry,
        indoc! {r#"
	# Fetch a state_entry in this hint and validate it in the update that comes next.
	ids.state_entry = __dict_manager.get_dict(ids.contract_state_changes)[
	    ids.contract_address
	]"#
        }
    ),
    (IsDeprecated, is_deprecated, "memory[ap] = to_felt_or_relocatable(is_deprecated)"),
    (
        EnterSyscallScopes,
        enter_syscall_scopes,
        indoc! {r#"vm_enter_scope({
        '__deprecated_class_hashes': __deprecated_class_hashes,
        'transactions': iter(block_input.transactions),
        'component_hashes': block_input.declared_class_hash_to_component_hashes,
        'execution_helper': execution_helper,
        'deprecated_syscall_handler': deprecated_syscall_handler,
        'syscall_handler': syscall_handler,
         '__dict_manager': __dict_manager,
    })"#
        }
    ),
    (
        IsRemainingGasLtInitialBudget,
        is_remaining_gas_lt_initial_budget,
        "memory[ap] = to_felt_or_relocatable(ids.remaining_gas < ids.ENTRY_POINT_INITIAL_BUDGET)"
    ),
    (
        InitialGeRequiredGas,
        initial_ge_required_gas,
        "memory[ap] = to_felt_or_relocatable(ids.initial_gas >= ids.required_gas)"
    ),
    (EnterScopeNode, enter_scope_node, "vm_enter_scope(dict(node=node, **common_args))"),
    (
        EnterScopeNewNode,
        enter_scope_new_node,
        indoc! {r#"
	ids.child_bit = 0 if case == 'left' else 1
	new_node = left_child if case == 'left' else right_child
	vm_enter_scope(dict(node=new_node, **common_args))"#
        }
    ),
    (
        EnterScopeNextNodeBit0,
        enter_scope_next_node_bit_0,
        indoc! {r#"
	new_node = left_child if ids.bit == 0 else right_child
	vm_enter_scope(dict(node=new_node, **common_args))"#
        }
    ),
    (
        EnterScopeNextNodeBit1,
        enter_scope_next_node_bit_1,
        indoc! {r#"
	new_node = left_child if ids.bit == 1 else right_child
	vm_enter_scope(dict(node=new_node, **common_args))"#
        }
    ),
    (
        EnterScopeLeftChild,
        enter_scope_left_child,
        "vm_enter_scope(dict(node=left_child, **common_args))"
    ),
    (
        EnterScopeRightChild,
        enter_scope_right_child,
        "vm_enter_scope(dict(node=right_child, **common_args))"
    ),
    (
        EnterScopeDescendEdge,
        enter_scope_descend_edge,
        indoc! {r#"
	new_node = node
	for i in range(ids.length - 1, -1, -1):
	    new_node = new_node[(ids.word >> i) & 1]
	vm_enter_scope(dict(node=new_node, **common_args))"#
        }
    ),
    (
        FetchResult,
        fetch_result,
        indoc! {r#"
    # Fetch the result, up to 100 elements.
    result = memory.get_range(ids.retdata, min(100, ids.retdata_size))

    if result != [ids.VALIDATED]:
        print("Invalid return value from __validate__:")
        print(f"  Size: {ids.retdata_size}")
        print(f"  Result (at most 100 elements): {result}")"#
        }
    ),
    (
        SearchSortedOptimistic,
        search_sorted_optimistic,
        indoc! {r#"array_ptr = ids.array_ptr
    elm_size = ids.elm_size
    assert isinstance(elm_size, int) and elm_size > 0, \
        f'Invalid value for elm_size. Got: {elm_size}.'

    n_elms = ids.n_elms
    assert isinstance(n_elms, int) and n_elms >= 0, \
        f'Invalid value for n_elms. Got: {n_elms}.'
    if '__find_element_max_size' in globals():
        assert n_elms <= __find_element_max_size, \
            f'find_element() can only be used with n_elms<={__find_element_max_size}. ' \
            f'Got: n_elms={n_elms}.'

    for i in range(n_elms):
        if memory[array_ptr + elm_size * i] >= ids.key:
            ids.index = i
            ids.exists = 1 if memory[array_ptr + elm_size * i] == ids.key else 0
            break
    else:
        ids.index = n_elms
        ids.exists = 0"#}
    ),
    (
        Log2Ceil,
        log2_ceil,
        indoc! {r#"from starkware.python.math_utils import log2_ceil
            ids.res = log2_ceil(ids.value)"#
        }
    ),
    (
        SetStateUpdatesStart,
        set_state_updates_start,
        indoc! {r#"# `use_kzg_da` is used in a hint in `process_data_availability`.
    use_kzg_da = ids.use_kzg_da
    if use_kzg_da or ids.compress_state_updates:
        ids.state_updates_start = segments.add()
    else:
        # Assign a temporary segment, to be relocated into the output segment.
        ids.state_updates_start = segments.add_temp_segment()"#}
    ),
    (
        SetCompressedStart,
        set_compressed_start,
        indoc! {r#"if use_kzg_da:
    ids.compressed_start = segments.add()
else:
    # Assign a temporary segment, to be relocated into the output segment.
    ids.compressed_start = segments.add_temp_segment()"#}
    ),
    (
        SetNUpdatesSmall,
        set_n_updates_small,
        indoc! {r#"ids.is_n_updates_small = ids.n_updates < ids.N_UPDATES_SMALL_PACKING_BOUND"#}
    ),
    (SetSiblings, set_siblings, "memory[ids.siblings], ids.word = descend"),
    (IsCaseRight, is_case_right, "memory[ap] = int(case == 'right') ^ ids.bit"),
    (
        SetApToDescend,
        set_ap_to_descend,
        indoc! {r#"
	descend = descent_map.get((ids.height, ids.path))
	memory[ap] = 0 if descend is None else 1"#
        }
    ),
    (AssertCaseIsRight, assert_case_is_right, "assert case == 'right'"),
    (
        WriteCaseNotLeftToAp,
        write_case_not_left_to_ap,
        indoc! {r#"
            memory[ap] = int(case != 'left')"#
        }
    ),
    (SplitDescend, split_descend, "ids.length, ids.word = descend"),
    (
        RemainingGasGtMax,
        remaining_gas_gt_max,
        "memory[ap] = to_felt_or_relocatable(ids.remaining_gas > ids.max_gas)"
    ),
    (
        DecodeNode,
        decode_node,
        indoc! {r#"
	from starkware.python.merkle_tree import decode_node
	left_child, right_child, case = decode_node(node)
	memory[ap] = int(case != 'both')"#
        }
    ),
    (
        DecodeNode2,
        decode_node,
        indoc! {r#"
from starkware.python.merkle_tree import decode_node
left_child, right_child, case = decode_node(node)
memory[ap] = 1 if case != 'both' else 0"#
        }
    ),
    (
        WriteSplitResult,
        write_split_result,
        indoc! {r#"
    from starkware.starknet.core.os.data_availability.bls_utils import split

    segments.write_arg(ids.res.address_, split(ids.value))"#
        }
    ),
    (IsOnCurve, is_on_curve, "ids.is_on_curve = (y * y) % SECP_P == y_square_int"),
    (
        StarknetOsInput,
        starknet_os_input,
        indoc! {r#"from starkware.starknet.core.os.os_hints import OsHintsConfig
        from starkware.starknet.core.os.os_input import StarknetOsInput

        os_input = StarknetOsInput.load(data=program_input)
        os_hints_config = OsHintsConfig.load(data=os_hints_config)
        block_input_iterator = iter(os_input.block_inputs)"#
        }
    ),
    (
        LogRemainingBlocks,
        log_remaining_blocks,
        indoc! {r#"print(f"execute_blocks: {ids.n_blocks} blocks remaining.")"#}
    ),
    (
        AllocateSegmentsForMessages,
        allocate_segments_for_messages,
        r#"# Allocate segments for the messages.
ids.initial_carried_outputs = segments.gen_arg(
    [segments.add_temp_segment(), segments.add_temp_segment()]
)"#
    ),
);

define_common_hint_enum!(
    CommonHint,
    (
        SetTreeStructure,
        set_tree_structure,
        indoc! {r#"from starkware.python.math_utils import div_ceil

    if __serialize_data_availability_create_pages__:
        onchain_data_start = ids.da_start
        onchain_data_size = ids.output_ptr - onchain_data_start

        # TODO(Yoni,20/07/2023): Take from input.
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
        ])"#}
    ),
    (
        GuessContractAddrStoragePtr,
        guess_contract_addr_storage_ptr,
        r#"if state_update_pointers is None:
    ids.squashed_prev_state = segments.add()
    ids.squashed_storage_ptr = segments.add()
else:
    ids.squashed_prev_state, ids.squashed_storage_ptr = (
        state_update_pointers.get_contract_state_entry_and_storage_ptr(
            contract_address=ids.state_changes.key
        )
    )"#
    ),
    (
        UpdateClassesPtr,
        update_classes_ptr,
        "if state_update_pointers is not None:
    state_update_pointers.class_tree_ptr = ids.squashed_dict_end.address_"
    ),
    (
        ComputeIdsLow,
        compute_ids_low,
        indoc! {r#"
            ids.low = (ids.value.d0 + ids.value.d1 * ids.BASE) & ((1 << 128) - 1)"#
        }
    ),
    (
        StoreDaSegment,
        store_da_segment,
        indoc! {r#"import itertools

    from starkware.python.utils import blockify

    kzg_manager.store_da_segment(
        da_segment=memory.get_range_as_ints(addr=ids.state_updates_start, size=ids.da_size)
    )
    kzg_commitments = [
        kzg_manager.polynomial_coefficients_to_kzg_commitment_callback(chunk)
        for chunk in blockify(kzg_manager.da_segment, chunk_size=ids.BLOB_LENGTH)
    ]

    ids.n_blobs = len(kzg_commitments)
    ids.kzg_commitments = segments.add_temp_segment()
    ids.evals = segments.add_temp_segment()

    segments.write_arg(ids.kzg_commitments.address_, list(itertools.chain(*kzg_commitments)))"#}
    ),
    (
        GuessClassesPtr,
        guess_classes_ptr,
        "if state_update_pointers is None:
    ids.squashed_dict = segments.add()
else:
    ids.squashed_dict = state_update_pointers.class_tree_ptr"
    ),
    (
        UpdateContractAddrToStoragePtr,
        update_contract_addr_to_storage_ptr,
        "if state_update_pointers is not None:
    state_update_pointers.contract_address_to_state_entry_and_storage_ptr[
        ids.state_changes.key
    ] = (
        ids.squashed_new_state.address_,
        ids.squashed_storage_ptr_end.address_,
    )"
    ),
    (
        SetStateUpdatePointersToNone,
        set_state_update_pointers_to_none,
        r#"state_update_pointers = None"#
    )
);

define_hint_enum!(
    OsHint,
    SnosHintProcessor<'_, S>,
    S,
    StateReader,
    (
        LoadClass,
        load_class,
        indoc! {r#"
    vm_exit_scope()

    computed_hash = ids.hash
    expected_hash = ids.compiled_class_fact.hash
    assert computed_hash == expected_hash, (
        "Computed compiled_class_hash is inconsistent with the hash in the os_input. "
        f"Computed hash = {computed_hash}, Expected hash = {expected_hash}.")"#
        }
    ),
    (
        BytecodeSegmentStructure,
        bytecode_segment_structure,
        indoc! {r#"
    vm_enter_scope({
        "bytecode_segment_structure": bytecode_segment_structures[ids.compiled_class_fact.hash],
        "is_segment_used_callback": is_segment_used_callback
    })"#}
    ),
    (
        BlockNumber,
        block_number,
        "memory[ap] = to_felt_or_relocatable(syscall_handler.block_info.block_number)"
    ),
    (
        BlockTimestamp,
        block_timestamp,
        "memory[ap] = to_felt_or_relocatable(syscall_handler.block_info.block_timestamp)"
    ),
    (
        ChainId,
        chain_id,
        "memory[ap] = to_felt_or_relocatable(os_hints_config.starknet_os_config.chain_id)"
    ),
    (
        FeeTokenAddress,
        fee_token_address,
        "memory[ap] = to_felt_or_relocatable(os_hints_config.starknet_os_config.fee_token_address)"
    ),
    (
        SequencerAddress,
        sequencer_address,
        "memory[ap] = to_felt_or_relocatable(syscall_handler.block_info.sequencer_address)"
    ),
    (
        WriteUseKzgDaToMemory,
        write_use_kzg_da_to_memory,
        indoc! {r#"memory[fp + 19] = to_felt_or_relocatable(os_hints_config.use_kzg_da and (
    not os_hints_config.full_output
))"#}
    ),
    (
        UpdateBuiltinPtrs,
        update_builtin_ptrs,
        indoc! {r#"
    from starkware.starknet.core.os.os_utils import update_builtin_pointers

    # Fill the values of all builtin pointers after the current transaction.
    ids.return_builtin_ptrs = segments.gen_arg(
        update_builtin_pointers(
            memory=memory,
            n_builtins=ids.n_builtins,
            builtins_encoding_addr=ids.builtin_params.builtin_encodings.address_,
            n_selected_builtins=ids.n_selected_builtins,
            selected_builtins_encoding_addr=ids.selected_encodings,
            orig_builtin_ptrs_addr=ids.builtin_ptrs.selectable.address_,
            selected_builtin_ptrs_addr=ids.selected_ptrs,
            ),
        )"#
        }
    ),
    (
        ReadStorageKeyForRevert,
        read_storage_key_for_revert,
        "memory[ap] = to_felt_or_relocatable(storage.read(key=ids.storage_key))"
    ),
    (
        WriteStorageKeyForRevert,
        write_storage_key_for_revert,
        "storage.write(key=ids.storage_key, value=ids.value)"
    ),
    (
        ValidateCompiledClassFactsPostExecution,
        validate_compiled_class_facts_post_execution,
        indoc! {r#"
    from starkware.starknet.core.os.contract_class.compiled_class_hash import (
        BytecodeAccessOracle,
    )

    # Build the bytecode segment structures.
    bytecode_segment_structures = {
        compiled_hash: create_bytecode_segment_structure(
            bytecode=compiled_class.bytecode,
            bytecode_segment_lengths=compiled_class.bytecode_segment_lengths,
        ) for compiled_hash, compiled_class in sorted(os_input.compiled_classes.items())
    }
    bytecode_segment_access_oracle = BytecodeAccessOracle(is_pc_accessed_callback=is_accessed)
    vm_enter_scope({
        "bytecode_segment_structures": bytecode_segment_structures,
        "is_segment_used_callback": bytecode_segment_access_oracle.is_segment_used
    })"#}
    ),
    (
        ReadAliasFromKey,
        read_alias_from_key,
        "memory[fp + 0] = to_felt_or_relocatable(aliases.read(key=ids.key))"
    ),
    (
        WriteNextAliasFromKey,
        write_next_alias_from_key,
        "aliases.write(key=ids.key, value=ids.next_available_alias)"
    ),
    (
        ReadAliasCounter,
        read_alias_counter,
        "memory[ap] = to_felt_or_relocatable(aliases.read(key=ids.ALIAS_COUNTER_STORAGE_KEY))"
    ),
    (
        InitializeAliasCounter,
        initialize_alias_counter,
        "aliases.write(key=ids.ALIAS_COUNTER_STORAGE_KEY, value=ids.INITIAL_AVAILABLE_ALIAS)"
    ),
    (
        UpdateAliasCounter,
        update_alias_counter,
        "aliases.write(key=ids.ALIAS_COUNTER_STORAGE_KEY, value=ids.next_available_alias)"
    ),
    (
        GuessAliasesContractStoragePtr,
        guess_aliases_contract_storage_ptr,
        r#"if state_update_pointers is None:
    ids.prev_aliases_state_entry = segments.add()
    ids.squashed_aliases_storage_start = segments.add()
else:
    ids.prev_aliases_state_entry, ids.squashed_aliases_storage_start = (
        state_update_pointers.get_contract_state_entry_and_storage_ptr(
            ids.ALIAS_CONTRACT_ADDRESS
        )
    )"#
    ),
    (
        UpdateAliasesContractToStoragePtr,
        update_aliases_contract_to_storage_ptr,
        "if state_update_pointers is not None:
    state_update_pointers.contract_address_to_state_entry_and_storage_ptr[
            ids.ALIAS_CONTRACT_ADDRESS
        ] = (
            ids.new_aliases_state_entry.address_,
            ids.squashed_aliases_storage_end.address_,
        )"
    ),
    (
        GuessStatePtr,
        guess_state_ptr,
        "if state_update_pointers is None:
    ids.final_squashed_contract_state_changes_start = segments.add()
else:
    ids.final_squashed_contract_state_changes_start = (
        state_update_pointers.state_tree_ptr
    )"
    ),
    (
        UpdateStatePtr,
        update_state_ptr,
        "if state_update_pointers is not None:
    state_update_pointers.state_tree_ptr = (
        ids.final_squashed_contract_state_changes_end.address_
    )"
    ),
    (
        LoadDeprecatedClassFacts,
        load_deprecated_class_facts,
        indoc! {r##"
    # Creates a set of deprecated class hashes to distinguish calls to deprecated entry points.
    __deprecated_class_hashes=set(os_input.deprecated_compiled_classes.keys())
    ids.n_compiled_class_facts = len(os_input.deprecated_compiled_classes)
    vm_enter_scope({
        'compiled_class_facts': iter(sorted(os_input.deprecated_compiled_classes.items())),
    })"##
        }
    ),
    (
        LoadDeprecatedClassInner,
        load_deprecated_class_inner,
        indoc! {r#"
    from starkware.starknet.core.os.contract_class.deprecated_class_hash_cairo_utils import (
        get_deprecated_contract_class_struct,
    )

    compiled_class_hash, compiled_class = next(compiled_class_facts)

    cairo_contract = get_deprecated_contract_class_struct(
        identifiers=ids._context.identifiers, contract_class=compiled_class)
    ids.compiled_class = segments.gen_arg(cairo_contract)"#
        }
    ),
    (StartTx, start_tx, indoc! {r#"execution_helper.start_tx()"# }),
    (
        OsInputTransactions,
        os_input_transactions,
        indoc! {r#"memory[fp + 12] = to_felt_or_relocatable(len(block_input.transactions))"#
        }
    ),
    (
        SetApToActualFee,
        set_ap_to_actual_fee,
        indoc! {
            r#"memory[ap] = to_felt_or_relocatable(execution_helper.tx_execution_info.actual_fee)"#
        }
    ),
    (
        SkipTx,
        skip_tx,
        indoc! {r#"execution_helper.skip_tx()"#
        }
    ),
    (
        SetSha256SegmentInSyscallHandler,
        set_sha256_segment_in_syscall_handler,
        indoc! {r#"syscall_handler.sha256_segment = ids.sha256_ptr"#}
    ),
    (
        SetComponentHashes,
        set_component_hashes,
        indoc! {r#"
class_component_hashes = component_hashes[tx.class_hash]
assert (
    len(class_component_hashes) == ids.ContractClassComponentHashes.SIZE
), "Wrong number of class component hashes."
ids.contract_class_component_hashes = segments.gen_arg(class_component_hashes)"#
        }
    ),
    (
        LoadNextTx,
        load_next_tx,
        indoc! {r#"
    tx = next(transactions)
    assert tx.tx_type.name in ('INVOKE_FUNCTION', 'L1_HANDLER', 'DEPLOY_ACCOUNT', 'DECLARE'), (
        f"Unexpected transaction type: {tx.type.name}."
    )

    tx_type_bytes = tx.tx_type.name.encode("ascii")
    ids.tx_type = int.from_bytes(tx_type_bytes, "big")
    execution_helper.os_logger.enter_tx(
        tx=tx,
        n_steps=current_step,
        builtin_ptrs=ids.builtin_ptrs,
        range_check_ptr=ids.range_check_ptr,
    )

    # Prepare a short callable to save code duplication.
    exit_tx = lambda: execution_helper.os_logger.exit_tx(
        n_steps=current_step,
        builtin_ptrs=ids.builtin_ptrs,
        range_check_ptr=ids.range_check_ptr,
    )"#
        }
    ),
    (
        LoadResourceBounds,
        load_resource_bounds,
        indoc! {r#"
    from src.starkware.starknet.core.os.transaction_hash.transaction_hash import (
        create_resource_bounds_list,
    )
    assert len(tx.resource_bounds) == 3, (
        "Only transactions with 3 resource bounds are supported. "
        f"Got {len(tx.resource_bounds)} resource bounds."
    )
    ids.resource_bounds = segments.gen_arg(create_resource_bounds_list(tx.resource_bounds))"#
        }
    ),
    (ExitTx, exit_tx, "exit_tx()"),
    (
        PrepareConstructorExecution,
        prepare_constructor_execution,
        indoc! {r#"
    ids.contract_address_salt = tx.contract_address_salt
    ids.class_hash = tx.class_hash
    ids.constructor_calldata_size = len(tx.constructor_calldata)
    ids.constructor_calldata = segments.gen_arg(arg=tx.constructor_calldata)"#
        }
    ),
    (
        AssertTransactionHash,
        assert_transaction_hash,
        indoc! {r#"
    assert ids.transaction_hash == tx.hash_value, (
        "Computed transaction_hash is inconsistent with the hash in the transaction. "
        f"Computed hash = {ids.transaction_hash}, Expected hash = {tx.hash_value}.")"#
        }
    ),
    (
        SetStateEntryToAccountContractAddress,
        set_state_entry_to_account_contract_address,
        indoc! {r#"
    # Fetch a state_entry in this hint and validate it in the update that comes next.
    ids.state_entry = __dict_manager.get_dict(ids.contract_state_changes)[
        ids.tx_info.account_contract_address
    ]"#
        }
    ),
    (
        CheckIsDeprecated,
        check_is_deprecated,
        "is_deprecated = 1 if ids.execution_context.class_hash in __deprecated_class_hashes else 0"
    ),
    (EndTx, end_tx, "execution_helper.end_tx()"),
    (
        EnterCall,
        enter_call,
        indoc! {r#"
        execution_helper.enter_call(
            cairo_execution_info=ids.execution_context.execution_info,
            deprecated_tx_info=ids.execution_context.deprecated_tx_info,
        )"#}
    ),
    (ExitCall, exit_call, "execution_helper.exit_call()"),
    (
        ContractAddress,
        contract_address,
        indoc! {r#"
    from starkware.starknet.business_logic.transaction.deprecated_objects import (
        InternalL1Handler,
    )
    ids.contract_address = (
        tx.contract_address if isinstance(tx, InternalL1Handler) else tx.sender_address
    )"#
        }
    ),
    (TxCalldataLen, tx_calldata_len, "memory[ap] = to_felt_or_relocatable(len(tx.calldata))"),
    (TxCalldata, tx_calldata, "memory[ap] = to_felt_or_relocatable(segments.gen_arg(tx.calldata))"),
    (
        TxEntryPointSelector,
        tx_entry_point_selector,
        "memory[ap] = to_felt_or_relocatable(tx.entry_point_selector)"
    ),
    (TxVersion, tx_version, "memory[ap] = to_felt_or_relocatable(tx.version)"),
    (TxTip, tx_tip, "memory[ap] = to_felt_or_relocatable(tx.tip)"),
    (
        TxPaymasterDataLen,
        tx_paymaster_data_len,
        "memory[ap] = to_felt_or_relocatable(len(tx.paymaster_data))"
    ),
    (
        TxPaymasterData,
        tx_paymaster_data,
        "memory[ap] = to_felt_or_relocatable(segments.gen_arg(tx.paymaster_data))"
    ),
    (
        TxNonceDataAvailabilityMode,
        tx_nonce_data_availability_mode,
        "memory[ap] = to_felt_or_relocatable(tx.nonce_data_availability_mode)"
    ),
    (
        TxFeeDataAvailabilityMode,
        tx_fee_data_availability_mode,
        "memory[ap] = to_felt_or_relocatable(tx.fee_data_availability_mode)"
    ),
    (
        TxAccountDeploymentDataLen,
        tx_account_deployment_data_len,
        "memory[fp + 4] = to_felt_or_relocatable(len(tx.account_deployment_data))"
    ),
    (
        TxAccountDeploymentData,
        tx_account_deployment_data,
        "memory[ap] = to_felt_or_relocatable(segments.gen_arg(tx.account_deployment_data))"
    ),
    (
        GenSignatureArg,
        gen_signature_arg,
        indoc! {r#"
	ids.signature_start = segments.gen_arg(arg=tx.signature)
	ids.signature_len = len(tx.signature)"#
        }
    ),
    (
        IsReverted,
        is_reverted,
        "memory[ap] = to_felt_or_relocatable(execution_helper.tx_execution_info.is_reverted)"
    ),
    (
        CheckExecution,
        check_execution,
        indoc! {r#"
    if execution_helper.debug_mode:
        # Validate the predicted gas cost.
        # TODO(Yoni, 1/1/2025): remove this check once Cairo 0 is not supported.
        actual = ids.remaining_gas - ids.entry_point_return_values.gas_builtin
        predicted = execution_helper.call_info.gas_consumed
        if execution_helper.call_info.tracked_resource.is_sierra_gas():
            predicted = predicted - ids.ENTRY_POINT_INITIAL_BUDGET
            assert actual == predicted, (
                "Predicted gas costs are inconsistent with the actual execution; "
                f"{predicted=}, {actual=}."
            )
        else:
            assert predicted == 0, "Predicted gas cost must be zero in CairoSteps mode."


    # Exit call.
    syscall_handler.validate_and_discard_syscall_ptr(
        syscall_ptr_end=ids.entry_point_return_values.syscall_ptr
    )
    execution_helper.exit_call()"#
        }
    ),
    (
        CheckSyscallResponse,
        check_syscall_response,
        indoc! {r#"
	# Check that the actual return value matches the expected one.
	expected = memory.get_range(
	    addr=ids.call_response.retdata, size=ids.call_response.retdata_size
	)
	actual = memory.get_range(addr=ids.retdata, size=ids.retdata_size)

	assert expected == actual, f'Return value mismatch expected={expected}, actual={actual}.'"#
        }
    ),
    (
        CheckNewSyscallResponse,
        check_new_syscall_response,
        indoc! {r#"
	# Check that the actual return value matches the expected one.
	expected = memory.get_range(
	    addr=ids.response.retdata_start,
	    size=ids.response.retdata_end - ids.response.retdata_start,
	)
	actual = memory.get_range(addr=ids.retdata, size=ids.retdata_size)

	assert expected == actual, f'Return value mismatch; expected={expected}, actual={actual}.'"#
        }
    ),
    (
        CheckNewDeployResponse,
        check_new_deploy_response,
        indoc! {r#"
	# Check that the actual return value matches the expected one.
	expected = memory.get_range(
	    addr=ids.response.constructor_retdata_start,
	    size=ids.response.constructor_retdata_end - ids.response.constructor_retdata_start,
	)
	actual = memory.get_range(addr=ids.retdata, size=ids.retdata_size)
	assert expected == actual, f'Return value mismatch; expected={expected}, actual={actual}.'"#
        }
    ),
    (
        LogEnterSyscall,
        log_enter_syscall,
        indoc! {r#"
    execution_helper.os_logger.enter_syscall(
        n_steps=current_step,
        builtin_ptrs=ids.builtin_ptrs,
        range_check_ptr=ids.range_check_ptr,
        deprecated=False,
        selector=ids.selector,
    )

    # Prepare a short callable to save code duplication.
    exit_syscall = lambda: execution_helper.os_logger.exit_syscall(
        n_steps=current_step,
        builtin_ptrs=ids.builtin_ptrs,
        range_check_ptr=ids.range_check_ptr,
        selector=ids.selector,
    )"#
        }
    ),
    (SetApToTxNonce, set_ap_to_tx_nonce, "memory[ap] = to_felt_or_relocatable(tx.nonce)"),
    (
        SetFpPlus4ToTxNonce,
        set_fp_plus_4_to_tx_nonce,
        "memory[fp + 4] = to_felt_or_relocatable(tx.nonce)"
    ),
    (
        WriteSyscallResultDeprecated,
        write_syscall_result_deprecated,
        indoc! {r#"
	storage = execution_helper.storage_by_address[ids.contract_address]
	ids.prev_value = storage.read(key=ids.syscall_ptr.address)
	storage.write(key=ids.syscall_ptr.address, value=ids.syscall_ptr.value)

	# Fetch a state_entry in this hint and validate it in the update that comes next.
	ids.state_entry = __dict_manager.get_dict(ids.contract_state_changes)[ids.contract_address]"#
        }
    ),
    (
        WriteSyscallResult,
        write_syscall_result,
        indoc! {r#"
    storage = execution_helper.storage_by_address[ids.contract_address]
    ids.prev_value = storage.read(key=ids.request.key)
    storage.write(key=ids.request.key, value=ids.request.value)

    # Fetch a state_entry in this hint and validate it in the update that comes next.
    ids.state_entry = __dict_manager.get_dict(ids.contract_state_changes)[ids.contract_address]"#
        }
    ),
    (
        DeclareTxFields,
        declare_tx_fields,
        indoc! {r#"
    assert tx.version == 3, f"Unsupported declare version: {tx.version}."
    ids.sender_address = tx.sender_address
    ids.account_deployment_data_size = len(tx.account_deployment_data)
    ids.account_deployment_data = segments.gen_arg(tx.account_deployment_data)
    ids.class_hash_ptr = segments.gen_arg([tx.class_hash])
    ids.compiled_class_hash = tx.compiled_class_hash"#
        }
    ),
    (
        WriteOldBlockToStorage,
        write_old_block_to_storage,
        indoc! {r#"
	storage = execution_helper.storage_by_address[ids.BLOCK_HASH_CONTRACT_ADDRESS]
	storage.write(key=ids.old_block_number, value=ids.old_block_hash)"#
        }
    ),
    (
        CacheContractStorageRequestKey,
        cache_contract_storage_request_key,
        indoc! {r#"
	# Make sure the value is cached (by reading it), to be used later on for the
	# commitment computation.
	value = execution_helper.storage_by_address[ids.contract_address].read(key=ids.request.key)
	assert ids.value == value, "Inconsistent storage value.""#
        }
    ),
    (
        CacheContractStorageSyscallRequestAddress,
        cache_contract_storage_syscall_request_address,
        indoc! {r#"
	# Make sure the value is cached (by reading it), to be used later on for the
	# commitment computation.
	value = execution_helper.storage_by_address[ids.contract_address].read(
	    key=ids.syscall_ptr.request.address
	)
	assert ids.value == value, "Inconsistent storage value.""#
        }
    ),
    (
        GetOldBlockNumberAndHash,
        get_old_block_number_and_hash,
        indoc! {r#"
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
        ids.old_block_hash = old_block_hash"#}
    ),
    (
        GetBlocksNumber,
        get_n_blocks,
        r#"memory[fp + 0] = to_felt_or_relocatable(len(os_input.block_inputs))"#
    ),
    (
        WriteFullOutputToMemory,
        write_full_output_to_memory,
        indoc! {r#"memory[fp + 20] = to_felt_or_relocatable(os_hints_config.full_output)"#}
    ),
    (
        ConfigureKzgManager,
        configure_kzg_manager,
        indoc! {r#"__serialize_data_availability_create_pages__ = True
        kzg_manager = global_hints.kzg_manager"#}
    ),
    (
        SetApToPrevBlockHash,
        set_ap_to_prev_block_hash,
        indoc! {r#"memory[ap] = to_felt_or_relocatable(block_input.prev_block_hash)"#}
    ),
    (
        SetApToNewBlockHash,
        set_ap_to_new_block_hash,
        "memory[ap] = to_felt_or_relocatable(block_input.new_block_hash)"
    ),
    (SetBit, set_bit, "ids.bit = (ids.edge.path >> ids.new_length) & 1"),
    (
        PreparePreimageValidationNonDeterministicHashes,
        prepare_preimage_validation_non_deterministic_hashes,
        indoc! {r#"
	from starkware.python.merkle_tree import decode_node
	left_child, right_child, case = decode_node(node)
	left_hash, right_hash = preimage[ids.node]

	# Fill non deterministic hashes.
	hash_ptr = ids.current_hash.address_
	memory[hash_ptr + ids.HashBuiltin.x] = left_hash
	memory[hash_ptr + ids.HashBuiltin.y] = right_hash

	if __patricia_skip_validation_runner:
	    # Skip validation of the preimage dict to speed up the VM. When this flag is set,
	    # mistakes in the preimage dict will be discovered only in the prover.
	    __patricia_skip_validation_runner.verified_addresses.add(
	        hash_ptr + ids.HashBuiltin.result)

	memory[ap] = int(case != 'both')"#
        }
    ),
    (
        BuildDescentMap,
        build_descent_map,
        indoc! {r#"
	from starkware.cairo.common.patricia_utils import canonic, patricia_guess_descents
	from starkware.python.merkle_tree import build_update_tree

	# Build modifications list.
	modifications = []
	DictAccess_key = ids.DictAccess.key
	DictAccess_new_value = ids.DictAccess.new_value
	DictAccess_SIZE = ids.DictAccess.SIZE
	for i in range(ids.n_updates):
	    curr_update_ptr = ids.update_ptr.address_ + i * DictAccess_SIZE
	    modifications.append((
	        memory[curr_update_ptr + DictAccess_key],
	        memory[curr_update_ptr + DictAccess_new_value]))

	node = build_update_tree(ids.height, modifications)
	descent_map = patricia_guess_descents(
	    ids.height, node, preimage, ids.prev_root, ids.new_root)
	del modifications
	__patricia_skip_validation_runner = globals().get(
	    '__patricia_skip_validation_runner')

	common_args = dict(
	    preimage=preimage, descent_map=descent_map,
	    __patricia_skip_validation_runner=__patricia_skip_validation_runner)
	common_args['common_args'] = common_args"#
        }
    ),
    (
        DebugExpectedInitialGas,
        debug_expected_initial_gas,
        indoc! {r#"
    if execution_helper.debug_mode:
        expected_initial_gas = execution_helper.call_info.call.initial_gas
        call_initial_gas = ids.remaining_gas
        assert expected_initial_gas == call_initial_gas, (
            f"Expected remaining_gas {expected_initial_gas}. Got: {call_initial_gas}.\n"
            f"{execution_helper.call_info=}"
        )"#}
    ),
    (
        IsSierraGasMode,
        is_sierra_gas_mode,
        "ids.is_sierra_gas_mode = execution_helper.call_info.tracked_resource.is_sierra_gas()"
    ),
    (
        ReadEcPointFromAddress,
        read_ec_point_from_address,
        r#"memory[ap] = to_felt_or_relocatable(ids.response.ec_point.address_ if ids.not_on_curve == 0 else segments.add())"#
    ),
    (
        SetPreimageForStateCommitments,
        set_preimage_for_state_commitments,
        indoc! {r#"ids.initial_root = block_input.contract_state_commitment_info.previous_root
ids.final_root = block_input.contract_state_commitment_info.updated_root
commitment_facts = block_input.contract_state_commitment_info.commitment_facts.items()
preimage = {
    int(root): children
    for root, children in commitment_facts
}
assert block_input.contract_state_commitment_info.tree_height == ids.MERKLE_HEIGHT"#
        }
    ),
    (
        SetPreimageForClassCommitments,
        set_preimage_for_class_commitments,
        indoc! {r#"ids.initial_root = block_input.contract_class_commitment_info.previous_root
ids.final_root = block_input.contract_class_commitment_info.updated_root
commitment_facts = block_input.contract_class_commitment_info.commitment_facts.items()
preimage = {
    int(root): children
    for root, children in commitment_facts
}
assert block_input.contract_class_commitment_info.tree_height == ids.MERKLE_HEIGHT"#
        }
    ),
    (
        SetPreimageForCurrentCommitmentInfo,
        set_preimage_for_current_commitment_info,
        indoc! {r#"commitment_info = commitment_info_by_address[ids.contract_address]
ids.initial_contract_state_root = commitment_info.previous_root
ids.final_contract_state_root = commitment_info.updated_root
preimage = {
    int(root): children
    for root, children in commitment_info.commitment_facts.items()
}
assert commitment_info.tree_height == ids.MERKLE_HEIGHT"#
        }
    ),
    (
        LoadEdge,
        load_edge,
        indoc! {r#"
	ids.edge = segments.add()
	ids.edge.length, ids.edge.path, ids.edge.bottom = preimage[ids.node]
	ids.hash_ptr.result = ids.node - ids.edge.length
	if __patricia_skip_validation_runner is not None:
	    # Skip validation of the preimage dict to speed up the VM. When this flag is set,
	    # mistakes in the preimage dict will be discovered only in the prover.
	    __patricia_skip_validation_runner.verified_addresses.add(
	        ids.hash_ptr + ids.HashBuiltin.result)"#
        }
    ),
    (
        LoadBottom,
        load_bottom,
        indoc! {r#"
	ids.hash_ptr.x, ids.hash_ptr.y = preimage[ids.edge.bottom]
	if __patricia_skip_validation_runner:
	    # Skip validation of the preimage dict to speed up the VM. When this flag is
	    # set, mistakes in the preimage dict will be discovered only in the prover.
	    __patricia_skip_validation_runner.verified_addresses.add(
	        ids.hash_ptr + ids.HashBuiltin.result)"#
        }
    ),
    (
        HeightIsZeroOrLenNodePreimageIsTwo,
        height_is_zero_or_len_node_preimage_is_two,
        "memory[ap] = 1 if ids.height == 0 or len(preimage[ids.node]) == 2 else 0"
    ),
    (
        SetSyscallPtr,
        set_syscall_ptr,
        indoc! {r#"
        syscall_handler.set_syscall_ptr(syscall_ptr=ids.syscall_ptr)"#
        }
    ),
    (
        OsLoggerEnterSyscallPrepareExitSyscall,
        os_logger_enter_syscall_prepare_exit_syscall,
        indoc! {r#"
    execution_helper.os_logger.enter_syscall(
        n_steps=current_step,
        builtin_ptrs=ids.builtin_ptrs,
        deprecated=True,
        selector=ids.selector,
        range_check_ptr=ids.range_check_ptr,
    )

    # Prepare a short callable to save code duplication.
    exit_syscall = lambda: execution_helper.os_logger.exit_syscall(
        n_steps=current_step,
        builtin_ptrs=ids.builtin_ptrs,
        range_check_ptr=ids.range_check_ptr,
        selector=ids.selector,
    )"#
        }
    ),
    (OsLoggerExitSyscall, os_logger_exit_syscall, "exit_syscall()"),
    (
        InitStateUpdatePointers,
        init_state_update_pointer,
        indoc! {r#"from starkware.starknet.core.os.execution_helper import StateUpdatePointers
        state_update_pointers = StateUpdatePointers(segments=segments)"#
        }
    ),
    (
        InitializeStateChanges,
        initialize_state_changes,
        indoc! {r#"from starkware.python.utils import from_bytes

initial_dict = {
    address: segments.gen_arg(
        (from_bytes(contract.contract_hash), segments.add(), contract.nonce))
    for address, contract in sorted(block_input.contracts.items())
}"#
        }
    ),
    (
        InitializeClassHashes,
        initialize_class_hashes,
        "initial_dict = block_input.class_hash_to_compiled_class_hash"
    ),
    (
        CreateBlockAdditionalHints,
        create_block_additional_hints,
        indoc! {r#"from starkware.starknet.core.os.os_hints import get_execution_helper_and_syscall_handlers
block_input = next(block_input_iterator)
(
    execution_helper,
    syscall_handler,
    deprecated_syscall_handler
) = get_execution_helper_and_syscall_handlers(
    block_input=block_input, global_hints=global_hints, os_hints_config=os_hints_config
)"#}
    )
);

define_hint_enum!(
    AggregatorHint,
    AggregatorHintProcessor<'_>,
    (
        DisableDaPageCreation,
        disable_da_page_creation,
        r#"# Note that `serialize_os_output` splits its output to memory pages
# (see OutputBuiltinRunner.add_page).
# Since this output is only used internally and will not be used in the final fact,
# we need to disable page creation.
__serialize_data_availability_create_pages__ = False"#
    ),
    (
        GetOsOuputForInnerBlocks,
        get_os_output_for_inner_blocks,
        r#"from starkware.starknet.core.aggregator.output_parser import parse_bootloader_output
from starkware.starknet.core.aggregator.utils import OsOutputToCairo

tasks = parse_bootloader_output(program_input["bootloader_output"])
assert len(tasks) > 0, "No tasks found in the bootloader output."
ids.os_program_hash = tasks[0].program_hash
ids.n_tasks = len(tasks)
os_output_to_cairo = OsOutputToCairo(segments)
for i, task in enumerate(tasks):
    os_output_to_cairo.process_os_output(
        segments=segments,
        dst_ptr=ids.os_outputs[i].address_,
        os_output=task.os_output,
    )"#
    ),
    (
        GetAggregatorOutput,
        get_aggregator_output,
        r#"from starkware.starknet.core.os.kzg_manager import KzgManager

__serialize_data_availability_create_pages__ = True
if "polynomial_coefficients_to_kzg_commitment_callback" not in globals():
    from services.utils import kzg_utils
    polynomial_coefficients_to_kzg_commitment_callback = (
        kzg_utils.polynomial_coefficients_to_kzg_commitment
    )
kzg_manager = KzgManager(polynomial_coefficients_to_kzg_commitment_callback)"#
    ),
    (
        WriteDaSegment,
        write_da_segment,
        r#"import json

da_path = program_input.get("da_path")
if da_path is not None:
    da_segment = kzg_manager.da_segment if program_input["use_kzg_da"] else None
    with open(da_path, "w") as da_file:
        json.dump(da_segment, da_file)"#
    ),
    (
        GetFullOutputFromInput,
        get_full_output_from_input,
        r#"memory[ap] = to_felt_or_relocatable(program_input["full_output"])"#
    ),
    (
        GetUseKzgDaFromInput,
        get_use_kzg_da_from_input,
        r#"memory[ap] = to_felt_or_relocatable(program_input["use_kzg_da"])"#
    ),
);

define_hint_extension_enum!(
    HintExtension,
    (
        LoadDeprecatedClass,
        load_deprecated_class,
        indoc! {r#"
    from starkware.python.utils import from_bytes

    computed_hash = ids.compiled_class_fact.hash
    expected_hash = compiled_class_hash
    assert computed_hash == expected_hash, (
        "Computed compiled_class_hash is inconsistent with the hash in the os_input. "
        f"Computed hash = {computed_hash}, Expected hash = {expected_hash}.")

    vm_load_program(compiled_class.program, ids.compiled_class.bytecode_ptr)"#
        }
    ),
    (
        LoadClassInner,
        load_class_inner,
        indoc! {r#"
    from starkware.starknet.core.os.contract_class.compiled_class_hash import (
        create_bytecode_segment_structure,
    )
    from starkware.starknet.core.os.contract_class.compiled_class_hash_cairo_hints import (
        get_compiled_class_struct,
    )

    ids.n_compiled_class_facts = len(os_input.compiled_classes)
    ids.compiled_class_facts = segments.add()
    for i, (compiled_class_hash, compiled_class) in enumerate(
        sorted(os_input.compiled_classes.items())
    ):
        # Load the compiled class.
        cairo_contract = get_compiled_class_struct(
            identifiers=ids._context.identifiers,
            compiled_class=compiled_class,
            # Load the entire bytecode - the unaccessed segments will be overriden and skipped
            # after the execution, in `validate_compiled_class_facts_post_execution`.
            bytecode=compiled_class.bytecode,
        )
        segments.load_data(
            ptr=ids.compiled_class_facts[i].address_,
            data=(compiled_class_hash, segments.gen_arg(cairo_contract))
        )

        bytecode_ptr = ids.compiled_class_facts[i].compiled_class.bytecode_ptr
        # Compiled classes are expected to end with a `ret` opcode followed by a pointer to
        # the builtin costs.
        segments.load_data(
            ptr=bytecode_ptr + cairo_contract.bytecode_length,
            data=[0x208b7fff7fff7ffe, ids.builtin_costs]
        )

        # Load hints and debug info.
        vm_load_program(
            compiled_class.get_runnable_program(entrypoint_builtins=[]), bytecode_ptr)"#}
    ),
);
