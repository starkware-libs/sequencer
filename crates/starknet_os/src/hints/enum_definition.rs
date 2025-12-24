use blockifier::state::state_api::StateReader;
use indoc::indoc;
#[cfg(any(test, feature = "testing"))]
use serde::{Deserialize, Serialize};
use starknet_types_core::hash::{Blake2Felt252, Poseidon};
#[cfg(any(test, feature = "testing"))]
use strum::IntoEnumIterator;

use crate::hint_processor::aggregator_hint_processor::AggregatorHintProcessor;
use crate::hint_processor::common_hint_processor::CommonHintProcessor;
use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::{OsHintError, OsHintExtensionResult, OsHintResult};
use crate::hints::hint_implementation::aggregator::implementation::{
    allocate_segments_for_messages,
    disable_da_page_creation,
    get_aggregator_output,
    get_chain_id_from_input,
    get_fee_token_address_from_input,
    get_full_output_from_input,
    get_os_output_for_inner_blocks,
    get_public_keys_from_aggregator_input,
    get_use_kzg_da_from_input,
    set_state_update_pointers_to_none,
    write_da_segment,
};
use crate::hints::hint_implementation::blake2s::implementation::naive_unpack_felt252_to_u32s;
use crate::hints::hint_implementation::block_context::{
    block_number_timestamp_and_address,
    chain_id_and_fee_token_address,
    get_block_hash_mapping,
};
use crate::hints::hint_implementation::bls_field::implementation::compute_ids_low;
use crate::hints::hint_implementation::builtins::{
    select_builtin,
    selected_builtins,
    update_builtin_ptrs,
};
use crate::hints::hint_implementation::cairo1_revert::implementation::{
    generate_dummy_os_output_segment,
    prepare_state_entry_for_revert,
    read_storage_key_for_revert,
    write_storage_key_for_revert,
};
use crate::hints::hint_implementation::compiled_class::implementation::{
    assert_end_of_bytecode_segments,
    assign_bytecode_segments,
    delete_memory_data,
    enter_scope_with_bytecode_segment_structure,
    is_leaf,
    iter_current_segment_info,
    load_class,
    load_classes_and_create_bytecode_segment_structures,
    set_ap_to_segment_hash,
};
use crate::hints::hint_implementation::deprecated_compiled_class::implementation::{
    load_deprecated_class,
    load_deprecated_class_facts,
    load_deprecated_class_inner,
};
use crate::hints::hint_implementation::execute_syscalls::{
    is_block_number_in_block_hash_buffer,
    relocate_sha256_segment,
};
use crate::hints::hint_implementation::execute_transactions::implementation::{
    fill_holes_in_rc96_segment,
    load_actual_fee,
    log_remaining_txs,
    os_input_transactions,
    segments_add,
    segments_add_temp_initial_txs_range_check_ptr,
    set_component_hashes,
    sha2_finalize,
    skip_tx,
    start_tx,
};
use crate::hints::hint_implementation::execution::implementation::{
    assert_transaction_hash,
    cache_contract_storage_request_key,
    cache_contract_storage_syscall_request_address,
    check_execution_and_exit_call,
    check_is_deprecated,
    check_new_call_contract_response,
    check_new_deploy_response,
    check_retdata_for_debug,
    check_syscall_response,
    contract_address,
    declare_tx_fields,
    end_tx,
    enter_call,
    enter_scope_deprecated_syscall_handler,
    enter_scope_execute_transactions_inner,
    enter_scope_syscall_handler,
    exit_call,
    exit_tx,
    gen_signature_arg,
    get_contract_address_state_entry,
    get_old_block_number_and_hash,
    initial_ge_required_gas,
    is_remaining_gas_lt_initial_budget,
    is_reverted,
    load_common_tx_fields,
    load_next_tx,
    prepare_constructor_execution,
    set_ap_to_tx_nonce,
    set_fp_to_tx_nonce,
    set_state_entry_to_account_contract_address,
    tx_account_deployment_data,
    tx_calldata,
    tx_entry_point_selector,
    tx_proof_facts,
    tx_version,
    write_old_block_to_storage,
    write_syscall_result,
    write_syscall_result_deprecated,
};
use crate::hints::hint_implementation::find_element::search_sorted_optimistic;
use crate::hints::hint_implementation::kzg::implementation::{
    guess_kzg_commitments_and_store_da_segment,
    write_split_result,
};
use crate::hints::hint_implementation::math::log2_ceil;
use crate::hints::hint_implementation::os::{
    check_block_hash_consistency,
    configure_kzg_manager,
    create_block_additional_hints,
    get_block_hashes,
    get_n_blocks,
    get_n_class_hashes_to_migrate,
    get_public_keys,
    init_state_update_pointer,
    initialize_class_hashes,
    initialize_state_changes,
    log_remaining_blocks,
    starknet_os_input,
    write_use_kzg_da_and_full_output_to_memory,
};
use crate::hints::hint_implementation::os_logger::{
    log_enter_syscall,
    os_logger_enter_syscall_prepare_exit_syscall,
    os_logger_exit_syscall,
};
use crate::hints::hint_implementation::output::{
    calculate_keys_using_sha256_hash,
    set_compressed_start,
    set_encrypted_start,
    set_n_updates_small,
    set_proof_fact_topology,
    set_state_updates_start,
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
    should_use_read_optimized_patricia_update,
    update_classes_ptr,
    update_state_ptr,
};
use crate::hints::hint_implementation::stateful_compression::implementation::{
    assert_key_big_enough_for_alias,
    contract_address_le_max_for_compression,
    enter_scope_with_aliases,
    get_class_hash_and_compiled_class_fact,
    guess_aliases_contract_storage_ptr,
    initialize_alias_counter,
    key_lt_min_alias_alloc_value,
    load_storage_ptr_and_prev_state,
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
        #[cfg_attr(
            any(test, feature = "testing"),
            derive(Deserialize, Serialize, Ord, PartialOrd, strum_macros::EnumIter)
        )]
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
    (IsBlockNumberInBlockHashBuffer, is_block_number_in_block_hash_buffer),
    (GetBlockHashMapping, get_block_hash_mapping),
    (IsLeaf, is_leaf),
    // Builtin selection hints are non-whitelisted hints that are part of cairo common.
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
    (PrepareStateEntryForRevert, prepare_state_entry_for_revert),
    (GenerateDummyOsOutputSegment, generate_dummy_os_output_segment),
    (AssignBytecodeSegments, assign_bytecode_segments),
    (AssertEndOfBytecodeSegments, assert_end_of_bytecode_segments),
    (DeleteMemoryData, delete_memory_data),
    (IterCurrentSegmentInfo, iter_current_segment_info),
    (SetApToSegmentHashPoseidon, set_ap_to_segment_hash::<Poseidon>),
    (SetApToSegmentHashBlake, set_ap_to_segment_hash::<Blake2Felt252>),
    (EnterScopeWithAliases, enter_scope_with_aliases),
    (KeyLtMinAliasAllocValue, key_lt_min_alias_alloc_value),
    (AssertKeyBigEnoughForAlias, assert_key_big_enough_for_alias),
    (ContractAddressLeMaxForCompression, contract_address_le_max_for_compression),
    (
        ComputeCommitmentsOnFinalizedStateWithAliases,
        compute_commitments_on_finalized_state_with_aliases
    ),
    (DictionaryFromBucket, dictionary_from_bucket),
    (GetPrevOffset, get_prev_offset),
    (CompressionHint, compression_hint),
    (SetDecompressedDst, set_decompressed_dst),
    (SegmentsAddTempInitialTxsRangeCheckPtr, segments_add_temp_initial_txs_range_check_ptr),
    (
        SegmentsAdd,
        segments_add,
        // Still used in cairo-lang (segment arena).
        indoc! {r#"memory[ap] = to_felt_or_relocatable(segments.add())"#
        }
    ),
    (LogRemainingTxs, log_remaining_txs),
    (FillHolesInRc96Segment, fill_holes_in_rc96_segment),
    // Non-whitelisted hints that is part of cairo common.
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
    (EnterScopeDeprecatedSyscallHandler, enter_scope_deprecated_syscall_handler),
    (EnterScopeSyscallHandler, enter_scope_syscall_handler),
    (GetContractAddressStateEntry, get_contract_address_state_entry),
    (EnterScopeExecuteTransactionsInner, enter_scope_execute_transactions_inner),
    (IsRemainingGasLtInitialBudget, is_remaining_gas_lt_initial_budget),
    (InitialGeRequiredGas, initial_ge_required_gas),
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
    (CheckRetdataForDebug, check_retdata_for_debug),
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
    (SetStateUpdatesStart, set_state_updates_start),
    (SetCompressedStart, set_compressed_start),
    (SetEncryptedStart, set_encrypted_start),
    (SetNUpdatesSmall, set_n_updates_small),
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
    (RemainingGasGtMax, remaining_gas_gt_max),
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
    (WriteSplitResult, write_split_result),
    (IsOnCurve, is_on_curve, "ids.is_on_curve = (y * y) % SECP_P == y_square_int"),
    (StarknetOsInput, starknet_os_input),
    (AllocateSegmentsForMessages, allocate_segments_for_messages),
    (NaiveUnpackFelt252ToU32s, naive_unpack_felt252_to_u32s),
    (GenerateKeysUsingSha256Hash, calculate_keys_using_sha256_hash)
);

define_common_hint_enum!(
    CommonHint,
    (SetProofFactTopology, set_proof_fact_topology),
    (LoadStoragePtrAndPrevState, load_storage_ptr_and_prev_state),
    (UpdateClassesPtr, update_classes_ptr),
    (ComputeIdsLow, compute_ids_low),
    (GuessKzgCommitmentsAndStoreDaSegment, guess_kzg_commitments_and_store_da_segment),
    (GuessClassesPtr, guess_classes_ptr),
    (UpdateContractAddrToStoragePtr, update_contract_addr_to_storage_ptr),
    (SetStateUpdatePointersToNone, set_state_update_pointers_to_none)
);

define_hint_enum!(
    OsHint,
    SnosHintProcessor<'_, S>,
    S,
    StateReader,
    (LoadClass, load_class),
    (RelocateSha256Segment, relocate_sha256_segment),
    (EnterScopeWithBytecodeSegmentStructure, enter_scope_with_bytecode_segment_structure),
    (BlockNumberTimestampAndAddress, block_number_timestamp_and_address),
    (ChainIdAndFeeTokenAddress, chain_id_and_fee_token_address),
    (WriteUseKzgDaAndFullOutputToMemory, write_use_kzg_da_and_full_output_to_memory),
    (UpdateBuiltinPtrs, update_builtin_ptrs),
    (ReadStorageKeyForRevert, read_storage_key_for_revert),
    (WriteStorageKeyForRevert, write_storage_key_for_revert),
    (ReadAliasFromKey, read_alias_from_key),
    (GetClassHashAndCompiledClassFact, get_class_hash_and_compiled_class_fact),
    (WriteNextAliasFromKey, write_next_alias_from_key),
    (ReadAliasCounter, read_alias_counter),
    (InitializeAliasCounter, initialize_alias_counter),
    (UpdateAliasCounter, update_alias_counter),
    (GuessAliasesContractStoragePtr, guess_aliases_contract_storage_ptr),
    (UpdateAliasesContractToStoragePtr, update_aliases_contract_to_storage_ptr),
    (GuessStatePtr, guess_state_ptr),
    (UpdateStatePtr, update_state_ptr),
    (LoadDeprecatedClassFacts, load_deprecated_class_facts),
    (LoadDeprecatedClassInner, load_deprecated_class_inner),
    (StartTx, start_tx),
    (OsInputTransactions, os_input_transactions),
    (LoadActualFee, load_actual_fee),
    (SkipTx, skip_tx),
    (SetComponentHashes, set_component_hashes),
    (LoadNextTx, load_next_tx),
    (LoadCommonTxFields, load_common_tx_fields),
    (ExitTx, exit_tx),
    (PrepareConstructorExecution, prepare_constructor_execution),
    (AssertTransactionHash, assert_transaction_hash),
    (SetStateEntryToAccountContractAddress, set_state_entry_to_account_contract_address),
    (CheckIsDeprecated, check_is_deprecated),
    (EndTx, end_tx),
    (EnterCall, enter_call),
    (ExitCall, exit_call),
    (ContractAddress, contract_address),
    (TxCalldata, tx_calldata),
    (TxEntryPointSelector, tx_entry_point_selector),
    (TxVersion, tx_version),
    (TxAccountDeploymentData, tx_account_deployment_data),
    (TxProofFacts, tx_proof_facts),
    (GenSignatureArg, gen_signature_arg),
    (
        IsReverted,
        is_reverted,
        "memory[ap] = to_felt_or_relocatable(execution_helper.tx_execution_info.is_reverted)"
    ),
    (CheckExecutionAndExitCall, check_execution_and_exit_call),
    (CheckSyscallResponse, check_syscall_response),
    (CheckNewCallContractResponse, check_new_call_contract_response),
    (CheckNewDeployResponse, check_new_deploy_response),
    (LogEnterSyscall, log_enter_syscall),
    (SetApToTxNonce, set_ap_to_tx_nonce, "memory[ap] = to_felt_or_relocatable(tx.nonce)"),
    (SetFpToTxNonce, set_fp_to_tx_nonce, "memory[fp + 5] = to_felt_or_relocatable(tx.nonce)"),
    (WriteSyscallResultDeprecated, write_syscall_result_deprecated),
    (WriteSyscallResult, write_syscall_result),
    (DeclareTxFields, declare_tx_fields),
    (WriteOldBlockToStorage, write_old_block_to_storage),
    (CacheContractStorageRequestKey, cache_contract_storage_request_key),
    (CacheContractStorageSyscallRequestAddress, cache_contract_storage_syscall_request_address),
    (GetOldBlockNumberAndHash, get_old_block_number_and_hash),
    (
        GetBlocksNumber,
        get_n_blocks,
        r#"memory[fp + 3] = to_felt_or_relocatable(len(os_input.block_inputs))"#
    ),
    (GetNClassHashesToMigrate, get_n_class_hashes_to_migrate),
    (ConfigureKzgManager, configure_kzg_manager),
    (CheckBlockHashConsistency, check_block_hash_consistency),
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
    (DebugExpectedInitialGas, debug_expected_initial_gas),
    (IsSierraGasMode, is_sierra_gas_mode),
    (
        ReadEcPointFromAddress,
        read_ec_point_from_address,
        r#"memory[ap] = to_felt_or_relocatable(ids.response.ec_point.address_ if ids.not_on_curve == 0 else segments.add())"#
    ),
    (SetPreimageForStateCommitments, set_preimage_for_state_commitments),
    (SetPreimageForClassCommitments, set_preimage_for_class_commitments),
    (SetPreimageForCurrentCommitmentInfo, set_preimage_for_current_commitment_info),
    (ShouldUseReadOptimizedPatriciaUpdate, should_use_read_optimized_patricia_update),
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
    (SetSyscallPtr, set_syscall_ptr),
    (OsLoggerEnterSyscallPrepareExitSyscall, os_logger_enter_syscall_prepare_exit_syscall),
    (OsLoggerExitSyscall, os_logger_exit_syscall),
    (InitStateUpdatePointers, init_state_update_pointer),
    (InitializeStateChanges, initialize_state_changes),
    (InitializeClassHashes, initialize_class_hashes),
    (CreateBlockAdditionalHints, create_block_additional_hints),
    (LogRemainingBlocks, log_remaining_blocks),
    (GetPublicKeys, get_public_keys),
    (GetBlockHashes, get_block_hashes),
);

define_hint_enum!(
    AggregatorHint,
    AggregatorHintProcessor<'_>,
    (DisableDaPageCreation, disable_da_page_creation),
    (GetOsOuputForInnerBlocks, get_os_output_for_inner_blocks),
    (GetAggregatorOutput, get_aggregator_output),
    (WriteDaSegment, write_da_segment),
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
    (
        GetChainIdFromInput,
        get_chain_id_from_input,
        r#"memory[ap] = to_felt_or_relocatable(program_input["chain_id"])"#
    ),
    (
        GetFeeTokenAddressFromInput,
        get_fee_token_address_from_input,
        r#"memory[ap] = to_felt_or_relocatable(program_input["fee_token_address"])"#
    ),
    (GetPublicKeysFromAggregatorInput, get_public_keys_from_aggregator_input),
);

define_hint_extension_enum!(
    HintExtension,
    (LoadDeprecatedClass, load_deprecated_class),
    (
        LoadClassesAndBuildBytecodeSegmentStructures,
        load_classes_and_create_bytecode_segment_structures
    ),
);
