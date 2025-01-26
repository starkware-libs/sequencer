use std::collections::HashMap;

use cairo_vm::hint_processor::hint_processor_definition::{HintProcessor, HintReference};
use cairo_vm::serde::deserialize_program::ApTracking;
use cairo_vm::types::exec_scope::ExecutionScopes;
use cairo_vm::vm::vm_core::VirtualMachine;
use indoc::indoc;
use starknet_types_core::felt::Felt;

use crate::hints::block_context::{
    block_number,
    block_timestamp,
    bytecode_segment_structure,
    chain_id,
    deprecated_fee_token_address,
    elements_ge_10,
    elements_ge_2,
    fee_token_address,
    get_block_mapping,
    is_leaf,
    load_class,
    load_class_facts,
    load_class_inner,
    sequencer_address,
    write_use_kzg_da_to_memory,
};
use crate::hints::bls_field::compute_ids_low;
use crate::hints::builtins::{select_builtin, selected_builtins, update_builtin_ptrs};
use crate::hints::compiled_class::{
    assert_end_of_bytecode_segments,
    assign_bytecode_segments,
    iter_current_segment_info,
    set_ap_to_segment_hash,
};
use crate::hints::deprecated_compiled_class::{
    load_deprecated_class,
    load_deprecated_class_facts,
    load_deprecated_class_inner,
};
use crate::hints::error::{HintExtensionResult, HintResult, OsHintError};
use crate::hints::execute_transactions::{
    fill_holes_in_rc96_segment,
    log_remaining_txs,
    set_component_hashes,
    set_sha256_segment_in_syscall_handler,
    sha2_finalize,
    start_tx_validate_declare_execution_context,
};
use crate::hints::execution::{
    add_relocation_rule,
    assert_transaction_hash,
    cache_contract_storage_request_key,
    cache_contract_storage_syscall_request_address,
    check_execution,
    check_is_deprecated,
    check_new_deploy_response,
    check_new_syscall_response,
    check_syscall_response,
    contract_address,
    end_tx,
    enter_call,
    enter_scope_deprecated_syscall_handler,
    enter_scope_descend_edge,
    enter_scope_left_child,
    enter_scope_new_node,
    enter_scope_next_node_bit_0,
    enter_scope_next_node_bit_1,
    enter_scope_node,
    enter_scope_right_child,
    enter_scope_syscall_handler,
    enter_syscall_scopes,
    exit_call,
    exit_tx,
    fetch_result,
    gen_class_hash_arg,
    gen_signature_arg,
    get_block_hash_contract_address_state_entry_and_set_new_state_entry,
    get_contract_address_state_entry,
    get_contract_address_state_entry_and_set_new_state_entry,
    get_old_block_number_and_hash,
    initial_ge_required_gas,
    is_deprecated,
    is_reverted,
    load_next_tx,
    log_enter_syscall,
    os_context_segments,
    prepare_constructor_execution,
    resource_bounds,
    set_ap_to_tx_nonce,
    set_fp_plus_4_to_tx_nonce,
    set_state_entry_to_account_contract_address,
    start_tx,
    transaction_version,
    tx_account_deployment_data,
    tx_account_deployment_data_len,
    tx_calldata,
    tx_calldata_len,
    tx_entry_point_selector,
    tx_fee_data_availability_mode,
    tx_max_fee,
    tx_nonce,
    tx_nonce_data_availability_mode,
    tx_paymaster_data,
    tx_paymaster_data_len,
    tx_resource_bounds_len,
    tx_tip,
    write_old_block_to_storage,
    write_syscall_result,
    write_syscall_result_deprecated,
};
use crate::hints::find_element::search_sorted_optimistic;
use crate::hints::kzg::store_da_segment;
use crate::hints::stateless_compression::{
    compression_hint,
    dictionary_from_bucket,
    get_prev_offset,
    set_decompressed_dst,
};
use crate::hints::types::{HintEnum, HintExtensionImplementation, HintImplementation};
use crate::{define_hint_enum, define_hint_extension_enum};

define_hint_enum!(
    Hint,
    (
        LoadClassFacts,
        load_class_facts,
        indoc! {r#"
    ids.compiled_class_facts = segments.add()
    ids.n_compiled_class_facts = len(os_input.compiled_classes)
    vm_enter_scope({
        'compiled_class_facts': iter(os_input.compiled_classes.items()),
        'compiled_class_visited_pcs': os_input.compiled_class_visited_pcs,
    })"#}
    ),
    (
        LoadClassInner,
        load_class_inner,
        indoc! {r#"
    from starkware.starknet.core.os.contract_class.compiled_class_hash import (
        create_bytecode_segment_structure,
        get_compiled_class_struct,
    )

    compiled_class_hash, compiled_class = next(compiled_class_facts)

    bytecode_segment_structure = create_bytecode_segment_structure(
        bytecode=compiled_class.bytecode,
        bytecode_segment_lengths=compiled_class.bytecode_segment_lengths,
        visited_pcs=compiled_class_visited_pcs[compiled_class_hash],
    )

    cairo_contract = get_compiled_class_struct(
        identifiers=ids._context.identifiers,
        compiled_class=compiled_class,
        bytecode=bytecode_segment_structure.bytecode_with_skipped_segments()
    )
    ids.compiled_class = segments.gen_arg(cairo_contract)"#}
    ),
    (
        BytecodeSegmentStructure,
        bytecode_segment_structure,
        indoc! {r#"
    vm_enter_scope({
        "bytecode_segment_structure": bytecode_segment_structure
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
        "memory[ap] = to_felt_or_relocatable(os_input.general_config.chain_id.value)"
    ),
    (
        FeeTokenAddress,
        fee_token_address,
        "memory[ap] = to_felt_or_relocatable(os_input.general_config.fee_token_address)"
    ),
    (
        DeprecatedFeeTokenAddress,
        deprecated_fee_token_address,
        "memory[ap] = to_felt_or_relocatable(os_input.general_config.deprecated_fee_token_address)"
    ),
    (
        SequencerAddress,
        sequencer_address,
        "memory[ap] = to_felt_or_relocatable(syscall_handler.block_info.sequencer_address)"
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
        ElementsGe10,
        elements_ge_10,
        "memory[ap] = to_felt_or_relocatable(ids.elements_end - ids.elements >= 10)"
    ),
    (
        ElementsGe2,
        elements_ge_2,
        "memory[ap] = to_felt_or_relocatable(ids.elements_end - ids.elements >= 2)"
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
        WriteUseKzgDaToMemory,
        write_use_kzg_da_to_memory,
        indoc! {r#"
    memory[fp + 18] = to_felt_or_relocatable(syscall_handler.block_info.use_kzg_da and (
        not os_input.full_output
    ))"#}
    ),
    (
        ComputeIdsLow,
        compute_ids_low,
        indoc! {r#"
            ids.low = (ids.value.d0 + ids.value.d1 * ids.BASE) & ((1 << 128) - 1)"#
        }
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
        IterCurrentSegmentInfo,
        iter_current_segment_info,
        indoc! {r#"
    current_segment_info = next(bytecode_segments)

    is_used = current_segment_info.is_used
    ids.is_segment_used = 1 if is_used else 0

    is_used_leaf = is_used and isinstance(current_segment_info.inner_structure, BytecodeLeaf)
    ids.is_used_leaf = 1 if is_used_leaf else 0

    ids.segment_length = current_segment_info.segment_length
    vm_enter_scope(new_scope_locals={
        "bytecode_segment_structure": current_segment_info.inner_structure,
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
        LoadDeprecatedClassFacts,
        load_deprecated_class_facts,
        indoc! {r##"
    # Creates a set of deprecated class hashes to distinguish calls to deprecated entry points.
    __deprecated_class_hashes=set(os_input.deprecated_compiled_classes.keys())
    ids.compiled_class_facts = segments.add()
    ids.n_compiled_class_facts = len(os_input.deprecated_compiled_classes)
    vm_enter_scope({
        'compiled_class_facts': iter(os_input.deprecated_compiled_classes.items()),
    })"##
        }
    ),
    (
        LoadDeprecatedClassInner,
        load_deprecated_class_inner,
        indoc! {r#"
    from starkware.starknet.core.os.contract_class.deprecated_class_hash import (
        get_deprecated_contract_class_struct,
    )

    compiled_class_hash, compiled_class = next(compiled_class_facts)

    cairo_contract = get_deprecated_contract_class_struct(
        identifiers=ids._context.identifiers, contract_class=compiled_class)
    ids.compiled_class = segments.gen_arg(cairo_contract)"#
        }
    ),
    (
        StartTxValidateDeclareExecutionContext,
        start_tx_validate_declare_execution_context,
        indoc! {r#"
    execution_helper.start_tx(
        tx_info_ptr=ids.validate_declare_execution_context.deprecated_tx_info.address_
    )"#
        }
    ),
    (
        SetSha256SegmentInSyscallHandler,
        set_sha256_segment_in_syscall_handler,
        indoc! {r#"syscall_handler.sha256_segment = ids.sha256_ptr"#}
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
    (TransactionVersion, transaction_version, "memory[ap] = to_felt_or_relocatable(tx.version)"),
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
        GetBlockHashContractAddressStateEntryAndSetNewStateEntry,
        get_block_hash_contract_address_state_entry_and_set_new_state_entry,
        indoc! {r#"
	# Fetch a state_entry in this hint. Validate it in the update that comes next.
	ids.state_entry = __dict_manager.get_dict(ids.contract_state_changes)[
	    ids.BLOCK_HASH_CONTRACT_ADDRESS]
	ids.new_state_entry = segments.add()"#
        }
    ),
    (
        GetContractAddressStateEntryAndSetNewStateEntry,
        get_contract_address_state_entry_and_set_new_state_entry,
        indoc! {r#"
    # Fetch a state_entry in this hint and validate it in the update that comes next.
    ids.state_entry = __dict_manager.get_dict(ids.contract_state_changes)[ids.contract_address]
    ids.new_state_entry = segments.add()"#
        }
    ),
    (
        GetContractAddressStateEntryAndSetNewStateEntry2,
        get_contract_address_state_entry_and_set_new_state_entry,
        indoc! {r#"
	# Fetch a state_entry in this hint and validate it in the update that comes next.
	ids.state_entry = __dict_manager.get_dict(ids.contract_state_changes)[
	    ids.contract_address
	]

	ids.new_state_entry = segments.add()"#
        }
    ),
    (
        CheckIsDeprecated,
        check_is_deprecated,
        "is_deprecated = 1 if ids.execution_context.class_hash in __deprecated_class_hashes else 0"
    ),
    (IsDeprecated, is_deprecated, "memory[ap] = to_felt_or_relocatable(is_deprecated)"),
    (
        OsContextSegments,
        os_context_segments,
        indoc! {r#"
    ids.os_context = segments.add()
    ids.syscall_ptr = segments.add()"#
        }
    ),
    (
        EnterSyscallScopes,
        enter_syscall_scopes,
        indoc! {r#"vm_enter_scope({
        '__deprecated_class_hashes': __deprecated_class_hashes,
        'transactions': iter(os_input.transactions),
        'component_hashes': os_input.declared_class_hash_to_component_hashes,
        'execution_helper': execution_helper,
        'deprecated_syscall_handler': deprecated_syscall_handler,
        'syscall_handler': syscall_handler,
         '__dict_manager': __dict_manager,
    })"#
        }
    ),
    (EndTx, end_tx, "execution_helper.end_tx()"),
    (
        EnterCall,
        enter_call,
        indoc! {r#"
    execution_helper.enter_call(
        cairo_execution_info=ids.execution_context.execution_info)"#}
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
    (
        ResourceBounds,
        resource_bounds,
        indoc! {r#"
    from src.starkware.starknet.core.os.transaction_hash.transaction_hash import (
        create_resource_bounds_list,
    )

    ids.resource_bounds = (
        0
        if tx.version < 3
        else segments.gen_arg(create_resource_bounds_list(tx.resource_bounds))
    )"#
        }
    ),
    (
        TxMaxFee,
        tx_max_fee,
        "memory[ap] = to_felt_or_relocatable(tx.max_fee if tx.version < 3 else 0)"
    ),
    (TxNonce, tx_nonce, "memory[ap] = to_felt_or_relocatable(0 if tx.nonce is None else tx.nonce)"),
    (TxTip, tx_tip, "memory[ap] = to_felt_or_relocatable(0 if tx.version < 3 else tx.tip)"),
    (
        TxResourceBoundsLen,
        tx_resource_bounds_len,
        "memory[ap] = to_felt_or_relocatable(0 if tx.version < 3 else len(tx.resource_bounds))"
    ),
    (
        TxPaymasterDataLen,
        tx_paymaster_data_len,
        "memory[ap] = to_felt_or_relocatable(0 if tx.version < 3 else len(tx.paymaster_data))"
    ),
    (
        TxPaymasterData,
        tx_paymaster_data,
        "memory[ap] = to_felt_or_relocatable(0 if tx.version < 3 else \
         segments.gen_arg(tx.paymaster_data))"
    ),
    (
        TxNonceDataAvailabilityMode,
        tx_nonce_data_availability_mode,
        "memory[ap] = to_felt_or_relocatable(0 if tx.version < 3 else \
         tx.nonce_data_availability_mode)"
    ),
    (
        TxFeeDataAvailabilityMode,
        tx_fee_data_availability_mode,
        "memory[ap] = to_felt_or_relocatable(0 if tx.version < 3 else \
         tx.fee_data_availability_mode)"
    ),
    (
        TxAccountDeploymentDataLen,
        tx_account_deployment_data_len,
        "memory[ap] = to_felt_or_relocatable(0 if tx.version < 3 else \
         len(tx.account_deployment_data))"
    ),
    (
        TxAccountDeploymentData,
        tx_account_deployment_data,
        "memory[ap] = to_felt_or_relocatable(0 if tx.version < 3 else \
         segments.gen_arg(tx.account_deployment_data))"
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
        StartTx,
        start_tx,
        indoc! {r#"
    tx_info_ptr = ids.tx_execution_context.deprecated_tx_info.address_
    execution_helper.start_tx(tx_info_ptr=tx_info_ptr)"#
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
    return_values = ids.entry_point_return_values
    if return_values.failure_flag != 0:
        # Fetch the error, up to 100 elements.
        retdata_size = return_values.retdata_end - return_values.retdata_start
        error = memory.get_range(return_values.retdata_start, max(0, min(100, retdata_size)))

        print("Invalid return value in execute_entry_point:")
        print(f"  Class hash: {hex(ids.execution_context.class_hash)}")
        print(f"  Selector: {hex(ids.execution_context.execution_info.selector)}")
        print(f"  Size: {retdata_size}")
        print(f"  Error (at most 100 elements): {error}")

    if execution_helper.debug_mode:
        # Validate the predicted gas cost.
        actual = ids.remaining_gas - ids.entry_point_return_values.gas_builtin
        predicted = execution_helper.call_info.gas_consumed
        assert actual == predicted, (
            "Predicted gas costs are inconsistent with the actual execution; "
            f"{predicted=}, {actual=}."
        )

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
    exit_syscall = lambda selector: execution_helper.os_logger.exit_syscall(
        n_steps=current_step,
        builtin_ptrs=ids.builtin_ptrs,
        range_check_ptr=ids.range_check_ptr,
        selector=selector,
    )"#
        }
    ),
    (
        InitialGeRequiredGas,
        initial_ge_required_gas,
        "memory[ap] = to_felt_or_relocatable(ids.initial_gas >= ids.required_gas)"
    ),
    (
        AddRelocationRule,
        add_relocation_rule,
        "memory.add_relocation_rule(src_ptr=ids.src_ptr, dest_ptr=ids.dest_ptr)"
    ),
    (SetApToTxNonce, set_ap_to_tx_nonce, "memory[ap] = to_felt_or_relocatable(tx.nonce)"),
    (
        SetFpPlus4ToTxNonce,
        set_fp_plus_4_to_tx_nonce,
        "memory[fp + 4] = to_felt_or_relocatable(tx.nonce)"
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
        WriteSyscallResultDeprecated,
        write_syscall_result_deprecated,
        indoc! {r#"
	storage = execution_helper.storage_by_address[ids.contract_address]
	ids.prev_value = storage.read(key=ids.syscall_ptr.address)
	storage.write(key=ids.syscall_ptr.address, value=ids.syscall_ptr.value)

	# Fetch a state_entry in this hint and validate it in the update that comes next.
	ids.state_entry = __dict_manager.get_dict(ids.contract_state_changes)[ids.contract_address]

	ids.new_state_entry = segments.add()"#
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
    ids.state_entry = __dict_manager.get_dict(ids.contract_state_changes)[ids.contract_address]
    ids.new_state_entry = segments.add()"#
        }
    ),
    (
        GenClassHashArg,
        gen_class_hash_arg,
        indoc! {r#"
    ids.tx_version = tx.version
    ids.sender_address = tx.sender_address
    ids.class_hash_ptr = segments.gen_arg([tx.class_hash])
    if tx.version <= 1:
        assert tx.compiled_class_hash is None, (
            "Deprecated declare must not have compiled_class_hash."
        )
        ids.compiled_class_hash = 0
    else:
        assert tx.compiled_class_hash is not None, (
            "Declare must have a concrete compiled_class_hash."
        )
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
	(
	    old_block_number, old_block_hash
	) = execution_helper.get_old_block_number_and_hash()
	assert old_block_number == ids.old_block_number,(
	    "Inconsistent block number. "
	    "The constant STORED_BLOCK_HASH_BUFFER is probably out of sync."
	)
	ids.old_block_hash = old_block_hash"#
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
    )
);

define_hint_extension_enum!(
    HintExtension,
    (
        LoadClass,
        load_class,
        indoc! {r#"
    computed_hash = ids.compiled_class_fact.hash
    expected_hash = compiled_class_hash
    assert computed_hash == expected_hash, (
        "Computed compiled_class_hash is inconsistent with the hash in the os_input. "
        f"Computed hash = {computed_hash}, Expected hash = {expected_hash}.")

    vm_load_program(
        compiled_class.get_runnable_program(entrypoint_builtins=[]),
        ids.compiled_class.bytecode_ptr
    )"#
        }
    ),
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
    )
);
