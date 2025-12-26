use blockifier::state::state_api::StateReader;
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
    get_chain_id_and_fee_token_address_from_input,
    get_os_output_for_inner_blocks,
    get_public_keys_from_aggregator_input,
    get_use_kzg_da_and_full_output_from_input,
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
    load_tx_nonce,
    prepare_constructor_execution,
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
use crate::hints::pythonic_hint_strings::builtin_selection::{SELECTED_BUILTINS, SELECT_BUILTIN};
use crate::hints::pythonic_hint_strings::deprecated_syscalls::{
    CALL_CONTRACT,
    DELEGATE_CALL,
    DELEGATE_L1_HANDLER,
    DEPLOY,
    EMIT_EVENT,
    GET_BLOCK_NUMBER,
    GET_BLOCK_TIMESTAMP,
    GET_CALLER_ADDRESS,
    GET_CONTRACT_ADDRESS,
    GET_SEQUENCER_ADDRESS,
    GET_TX_INFO,
    GET_TX_SIGNATURE,
    LIBRARY_CALL,
    LIBRARY_CALL_L1_HANDLER,
    REPLACE_CLASS,
    SEND_MESSAGE_TO_L1,
    STORAGE_READ,
    STORAGE_WRITE,
};
use crate::hints::pythonic_hint_strings::find_element::SEARCH_SORTED_OPTIMISTIC;
use crate::hints::pythonic_hint_strings::math::LOG2_CEIL;
use crate::hints::pythonic_hint_strings::patricia::{
    ASSERT_CASE_IS_RIGHT,
    BUILD_DESCENT_MAP,
    DECODE_NODE,
    DECODE_NODE_2,
    ENTER_SCOPE_DESCEND_EDGE,
    ENTER_SCOPE_LEFT_CHILD,
    ENTER_SCOPE_NEW_NODE,
    ENTER_SCOPE_NEXT_NODE_BIT_0,
    ENTER_SCOPE_NEXT_NODE_BIT_1,
    ENTER_SCOPE_NODE,
    ENTER_SCOPE_RIGHT_CHILD,
    HEIGHT_IS_ZERO_OR_LEN_NODE_PREIMAGE_IS_TWO,
    IS_CASE_RIGHT,
    LOAD_BOTTOM,
    LOAD_EDGE,
    PREPARE_PREIMAGE_VALIDATION_NON_DETERMINISTIC_HASHES,
    SET_AP_TO_DESCEND,
    SET_BIT,
    SET_SIBLINGS,
    SPLIT_DESCEND,
    WRITE_CASE_NOT_LEFT_TO_AP,
};
use crate::hints::pythonic_hint_strings::secp::IS_ON_CURVE;
use crate::hints::pythonic_hint_strings::segment_arena::SEGMENTS_ADD;
use crate::hints::pythonic_hint_strings::sha256::SHA2_FINALIZE;
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
    (CallContract, call_contract, CALL_CONTRACT),
    (DelegateCall, delegate_call, DELEGATE_CALL),
    (DelegateL1Handler, delegate_l1_handler, DELEGATE_L1_HANDLER),
    (Deploy, deploy, DEPLOY),
    (EmitEvent, emit_event, EMIT_EVENT),
    (GetBlockNumber, get_block_number, GET_BLOCK_NUMBER),
    (GetBlockTimestamp, get_block_timestamp, GET_BLOCK_TIMESTAMP),
    (GetCallerAddress, get_caller_address, GET_CALLER_ADDRESS),
    (GetContractAddress, get_contract_address, GET_CONTRACT_ADDRESS),
    (GetSequencerAddress, get_sequencer_address, GET_SEQUENCER_ADDRESS),
    (GetTxInfo, get_tx_info, GET_TX_INFO),
    (GetTxSignature, get_tx_signature, GET_TX_SIGNATURE),
    (LibraryCall, library_call, LIBRARY_CALL),
    (LibraryCallL1Handler, library_call_l1_handler, LIBRARY_CALL_L1_HANDLER),
    (ReplaceClass, replace_class, REPLACE_CLASS),
    (SendMessageToL1, send_message_to_l1, SEND_MESSAGE_TO_L1),
    (StorageRead, storage_read, STORAGE_READ),
    (StorageWrite, storage_write, STORAGE_WRITE),
);

define_stateless_hint_enum!(
    StatelessHint,
    (IsBlockNumberInBlockHashBuffer, is_block_number_in_block_hash_buffer),
    (GetBlockHashMapping, get_block_hash_mapping),
    (IsLeaf, is_leaf),
    (SelectedBuiltins, selected_builtins, SELECTED_BUILTINS),
    (SelectBuiltin, select_builtin, SELECT_BUILTIN),
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
    (SegmentsAdd, segments_add, SEGMENTS_ADD),
    (LogRemainingTxs, log_remaining_txs),
    (FillHolesInRc96Segment, fill_holes_in_rc96_segment),
    (Sha2Finalize, sha2_finalize, SHA2_FINALIZE),
    (EnterScopeDeprecatedSyscallHandler, enter_scope_deprecated_syscall_handler),
    (EnterScopeSyscallHandler, enter_scope_syscall_handler),
    (GetContractAddressStateEntry, get_contract_address_state_entry),
    (EnterScopeExecuteTransactionsInner, enter_scope_execute_transactions_inner),
    (IsRemainingGasLtInitialBudget, is_remaining_gas_lt_initial_budget),
    (InitialGeRequiredGas, initial_ge_required_gas),
    (EnterScopeNode, enter_scope_node, ENTER_SCOPE_NODE),
    (EnterScopeNewNode, enter_scope_new_node, ENTER_SCOPE_NEW_NODE),
    (EnterScopeNextNodeBit0, enter_scope_next_node_bit_0, ENTER_SCOPE_NEXT_NODE_BIT_0),
    (EnterScopeNextNodeBit1, enter_scope_next_node_bit_1, ENTER_SCOPE_NEXT_NODE_BIT_1),
    (EnterScopeLeftChild, enter_scope_left_child, ENTER_SCOPE_LEFT_CHILD),
    (EnterScopeRightChild, enter_scope_right_child, ENTER_SCOPE_RIGHT_CHILD),
    (EnterScopeDescendEdge, enter_scope_descend_edge, ENTER_SCOPE_DESCEND_EDGE),
    (CheckRetdataForDebug, check_retdata_for_debug),
    (SearchSortedOptimistic, search_sorted_optimistic, SEARCH_SORTED_OPTIMISTIC),
    (Log2Ceil, log2_ceil, LOG2_CEIL),
    (SetStateUpdatesStart, set_state_updates_start),
    (SetCompressedStart, set_compressed_start),
    (SetEncryptedStart, set_encrypted_start),
    (SetNUpdatesSmall, set_n_updates_small),
    (SetSiblings, set_siblings, SET_SIBLINGS),
    (IsCaseRight, is_case_right, IS_CASE_RIGHT),
    (SetApToDescend, set_ap_to_descend, SET_AP_TO_DESCEND),
    (AssertCaseIsRight, assert_case_is_right, ASSERT_CASE_IS_RIGHT),
    (WriteCaseNotLeftToAp, write_case_not_left_to_ap, WRITE_CASE_NOT_LEFT_TO_AP),
    (SplitDescend, split_descend, SPLIT_DESCEND),
    (RemainingGasGtMax, remaining_gas_gt_max),
    (DecodeNode, decode_node, DECODE_NODE),
    (DecodeNode2, decode_node, DECODE_NODE_2),
    (WriteSplitResult, write_split_result),
    (IsOnCurve, is_on_curve, IS_ON_CURVE),
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
    (IsReverted, is_reverted),
    (CheckExecutionAndExitCall, check_execution_and_exit_call),
    (CheckSyscallResponse, check_syscall_response),
    (CheckNewCallContractResponse, check_new_call_contract_response),
    (CheckNewDeployResponse, check_new_deploy_response),
    (LogEnterSyscall, log_enter_syscall),
    (LoadTxNonce, load_tx_nonce),
    (WriteSyscallResultDeprecated, write_syscall_result_deprecated),
    (WriteSyscallResult, write_syscall_result),
    (DeclareTxFields, declare_tx_fields),
    (WriteOldBlockToStorage, write_old_block_to_storage),
    (CacheContractStorageRequestKey, cache_contract_storage_request_key),
    (CacheContractStorageSyscallRequestAddress, cache_contract_storage_syscall_request_address),
    (GetOldBlockNumberAndHash, get_old_block_number_and_hash),
    (GetBlocksNumber, get_n_blocks),
    (GetNClassHashesToMigrate, get_n_class_hashes_to_migrate),
    (ConfigureKzgManager, configure_kzg_manager),
    (CheckBlockHashConsistency, check_block_hash_consistency),
    (SetBit, set_bit, SET_BIT),
    (
        PreparePreimageValidationNonDeterministicHashes,
        prepare_preimage_validation_non_deterministic_hashes,
        PREPARE_PREIMAGE_VALIDATION_NON_DETERMINISTIC_HASHES
    ),
    (BuildDescentMap, build_descent_map, BUILD_DESCENT_MAP),
    (DebugExpectedInitialGas, debug_expected_initial_gas),
    (IsSierraGasMode, is_sierra_gas_mode),
    (ReadEcPointFromAddress, read_ec_point_from_address),
    (SetPreimageForStateCommitments, set_preimage_for_state_commitments),
    (SetPreimageForClassCommitments, set_preimage_for_class_commitments),
    (SetPreimageForCurrentCommitmentInfo, set_preimage_for_current_commitment_info),
    (ShouldUseReadOptimizedPatriciaUpdate, should_use_read_optimized_patricia_update),
    (LoadEdge, load_edge, LOAD_EDGE),
    (LoadBottom, load_bottom, LOAD_BOTTOM),
    (
        HeightIsZeroOrLenNodePreimageIsTwo,
        height_is_zero_or_len_node_preimage_is_two,
        HEIGHT_IS_ZERO_OR_LEN_NODE_PREIMAGE_IS_TWO
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
    (GetUseKzgDaAndFullOutputFromInput, get_use_kzg_da_and_full_output_from_input),
    (GetChainIdAndFeeTokenAddressFromInput, get_chain_id_and_fee_token_address_from_input),
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
