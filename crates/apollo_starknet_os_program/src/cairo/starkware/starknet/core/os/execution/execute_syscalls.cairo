from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.bool import FALSE
from starkware.cairo.common.builtin_keccak.keccak import (
    KECCAK_FULL_RATE_IN_WORDS,
    keccak_padded_input,
)
from starkware.cairo.common.cairo_builtins import BitwiseBuiltin, KeccakBuiltin
from starkware.cairo.common.cairo_secp.bigint import (
    bigint_to_uint256,
    nondet_bigint3,
    uint256_to_bigint,
)
from starkware.cairo.common.cairo_secp.bigint3 import BigInt3, UnreducedBigInt3
from starkware.cairo.common.cairo_secp.constants import SECP_PRIME_HIGH as SECP256K1_PRIME_HIGH
from starkware.cairo.common.cairo_secp.constants import SECP_PRIME_LOW as SECP256K1_PRIME_LOW
from starkware.cairo.common.cairo_secp.ec import ec_add as secp256k1_ec_add
from starkware.cairo.common.cairo_secp.ec import ec_mul_by_uint256 as secp256k1_ec_mul_by_uint256
from starkware.cairo.common.cairo_secp.ec_point import EcPoint as SecpPoint
from starkware.cairo.common.cairo_secp.field import (
    reduce,
    unreduced_mul,
    unreduced_sqr,
    validate_reduced_field_element,
    verify_zero,
)
from starkware.cairo.common.cairo_secp.signature import (
    try_get_point_from_x as secp256k1_try_get_point_from_x,
)
from starkware.cairo.common.dict import dict_read, dict_update
from starkware.cairo.common.dict_access import DictAccess
from starkware.cairo.common.math import (
    assert_le,
    assert_lt,
    assert_nn,
    assert_not_zero,
    unsigned_div_rem,
)
from starkware.cairo.common.memcpy import memcpy
from starkware.cairo.common.secp256r1.constants import SECP_PRIME_HIGH as SECP256R1_PRIME_HIGH
from starkware.cairo.common.secp256r1.constants import SECP_PRIME_LOW as SECP256R1_PRIME_LOW
from starkware.cairo.common.secp256r1.ec import ec_add as secp256r1_ec_add
from starkware.cairo.common.secp256r1.ec import ec_mul_by_uint256 as secp256r1_ec_mul_by_uint256
from starkware.cairo.common.secp256r1.ec import (
    try_get_point_from_x as secp256r1_try_get_point_from_x,
)
from starkware.cairo.common.segments import relocate_segment
from starkware.cairo.common.sha256_state import Sha256Input, Sha256ProcessBlock, Sha256State
from starkware.cairo.common.uint256 import Uint256, assert_uint256_lt, uint256_lt
from starkware.starknet.common.constants import ORIGIN_ADDRESS
from starkware.starknet.common.new_syscalls import (
    CALL_CONTRACT_SELECTOR,
    DEPLOY_SELECTOR,
    EMIT_EVENT_SELECTOR,
    GET_BLOCK_HASH_SELECTOR,
    GET_CLASS_HASH_AT_SELECTOR,
    GET_EXECUTION_INFO_SELECTOR,
    KECCAK_SELECTOR,
    LIBRARY_CALL_SELECTOR,
    META_TX_V0_SELECTOR,
    REPLACE_CLASS_SELECTOR,
    SECP256K1_ADD_SELECTOR,
    SECP256K1_GET_POINT_FROM_X_SELECTOR,
    SECP256K1_GET_XY_SELECTOR,
    SECP256K1_MUL_SELECTOR,
    SECP256K1_NEW_SELECTOR,
    SECP256R1_ADD_SELECTOR,
    SECP256R1_GET_POINT_FROM_X_SELECTOR,
    SECP256R1_GET_XY_SELECTOR,
    SECP256R1_MUL_SELECTOR,
    SECP256R1_NEW_SELECTOR,
    SEND_MESSAGE_TO_L1_SELECTOR,
    SHA256_PROCESS_BLOCK_SELECTOR,
    STORAGE_READ_SELECTOR,
    STORAGE_WRITE_SELECTOR,
    CallContractRequest,
    CallContractResponse,
    DeployRequest,
    DeployResponse,
    EmitEventRequest,
    ExecutionInfo,
    FailureReason,
    GetBlockHashRequest,
    GetBlockHashResponse,
    GetClassHashAtRequest,
    GetClassHashAtResponse,
    GetExecutionInfoResponse,
    KeccakRequest,
    KeccakResponse,
    LibraryCallRequest,
    MetaTxV0Request,
    ReplaceClassRequest,
    RequestHeader,
    ResourceBounds,
    ResponseHeader,
    Secp256k1AddRequest,
    Secp256k1AddResponse,
    Secp256k1GetPointFromXRequest,
    Secp256k1MulRequest,
    Secp256k1MulResponse,
    Secp256k1NewRequest,
    Secp256k1NewResponse,
    Secp256r1AddRequest,
    Secp256r1AddResponse,
    Secp256r1GetPointFromXRequest,
    Secp256r1MulRequest,
    Secp256r1MulResponse,
    Secp256r1NewRequest,
    Secp256r1NewResponse,
    SecpGetXyRequest,
    SecpGetXyResponse,
    SendMessageToL1Request,
    Sha256ProcessBlockRequest,
    Sha256ProcessBlockResponse,
    StorageReadRequest,
    StorageReadResponse,
    StorageWriteRequest,
    TxInfo,
)
from starkware.starknet.common.syscalls import TxInfo as DeprecatedTxInfo
from starkware.starknet.core.os.block_context import BlockContext
from starkware.starknet.core.os.builtins import (
    BuiltinPointers,
    NonSelectableBuiltins,
    SelectableBuiltins,
)
from starkware.starknet.core.os.constants import (
    BLOCK_HASH_CONTRACT_ADDRESS,
    CALL_CONTRACT_GAS_COST,
    CONSTRUCTOR_ENTRY_POINT_SELECTOR,
    DEPLOY_CALLDATA_FACTOR_GAS_COST,
    DEPLOY_GAS_COST,
    EMIT_EVENT_GAS_COST,
    ENTRY_POINT_TYPE_CONSTRUCTOR,
    ENTRY_POINT_TYPE_EXTERNAL,
    ERROR_BLOCK_NUMBER_OUT_OF_RANGE,
    ERROR_ENTRY_POINT_FAILED,
    ERROR_INVALID_ARGUMENT,
    ERROR_INVALID_INPUT_LEN,
    ERROR_OUT_OF_GAS,
    GET_BLOCK_HASH_GAS_COST,
    GET_CLASS_HASH_AT_GAS_COST,
    GET_EXECUTION_INFO_GAS_COST,
    KECCAK_GAS_COST,
    KECCAK_ROUND_COST_GAS_COST,
    LIBRARY_CALL_GAS_COST,
    META_TX_V0_CALLDATA_FACTOR_GAS_COST,
    META_TX_V0_GAS_COST,
    REPLACE_CLASS_GAS_COST,
    SECP256K1_ADD_GAS_COST,
    SECP256K1_GET_POINT_FROM_X_GAS_COST,
    SECP256K1_GET_XY_GAS_COST,
    SECP256K1_MUL_GAS_COST,
    SECP256K1_NEW_GAS_COST,
    SECP256R1_ADD_GAS_COST,
    SECP256R1_GET_POINT_FROM_X_GAS_COST,
    SECP256R1_GET_XY_GAS_COST,
    SECP256R1_MUL_GAS_COST,
    SECP256R1_NEW_GAS_COST,
    SEND_MESSAGE_TO_L1_GAS_COST,
    SHA256_PROCESS_BLOCK_GAS_COST,
    STORAGE_READ_GAS_COST,
    STORAGE_WRITE_GAS_COST,
    STORED_BLOCK_HASH_BUFFER,
    SYSCALL_BASE_GAS_COST,
)
from starkware.starknet.core.os.contract_address.contract_address import get_contract_address
from starkware.starknet.core.os.execution.account_backward_compatibility import (
    check_tip_for_v1_bound_accounts,
    exclude_data_gas_of_resource_bounds,
    is_v1_bound_account_cairo1,
    should_exclude_l1_data_gas,
)
from starkware.starknet.core.os.execution.deprecated_execute_entry_point import (
    select_execute_entry_point_func,
)
from starkware.starknet.core.os.execution.deprecated_execute_syscalls import deploy_contract
from starkware.starknet.core.os.execution.execute_entry_point import ExecutionContext
from starkware.starknet.core.os.execution.execute_transaction_utils import fill_deprecated_tx_info
from starkware.starknet.core.os.execution.revert import (
    CHANGE_CLASS_ENTRY,
    CHANGE_CONTRACT_ENTRY,
    RevertLogEntry,
)
from starkware.starknet.core.os.output import (
    MessageToL1Header,
    OsCarriedOutputs,
    os_carried_outputs_new,
)
from starkware.starknet.core.os.state.commitment import StateEntry
from starkware.starknet.core.os.transaction_hash.transaction_hash import (
    compute_meta_tx_v0_hash,
    update_pedersen_in_builtin_ptrs,
)

// Executes the system calls in syscall_ptr.
// The signature of the function 'call_execute_syscalls' must match this function's signature.
//
// Arguments:
// block_context - a read-only context used for transaction execution.
// execution_context - The execution context in which the system calls need to be executed.
// syscall_ptr_end - a pointer to the end of the syscall segment.
func execute_syscalls{
    range_check_ptr,
    syscall_ptr: felt*,
    builtin_ptrs: BuiltinPointers*,
    contract_state_changes: DictAccess*,
    contract_class_changes: DictAccess*,
    revert_log: RevertLogEntry*,
    outputs: OsCarriedOutputs*,
}(block_context: BlockContext*, execution_context: ExecutionContext*, syscall_ptr_end: felt*) {
    alloc_locals;
    if (syscall_ptr == syscall_ptr_end) {
        return ();
    }

    local selector = [syscall_ptr];
    %{
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
        )
    %}

    if (selector == STORAGE_READ_SELECTOR) {
        execute_storage_read(contract_address=execution_context.execution_info.contract_address);
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == STORAGE_WRITE_SELECTOR) {
        execute_storage_write(contract_address=execution_context.execution_info.contract_address);
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == GET_EXECUTION_INFO_SELECTOR) {
        execute_get_execution_info(execution_context=execution_context);
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == CALL_CONTRACT_SELECTOR) {
        execute_call_contract(
            block_context=block_context, caller_execution_context=execution_context
        );
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == LIBRARY_CALL_SELECTOR) {
        execute_library_call(
            block_context=block_context, caller_execution_context=execution_context
        );
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == EMIT_EVENT_SELECTOR) {
        // Skip as long as the block hash is not calculated by the OS.
        reduce_syscall_gas_and_write_response_header(
            total_gas_cost=EMIT_EVENT_GAS_COST, request_struct_size=EmitEventRequest.SIZE
        );
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == DEPLOY_SELECTOR) {
        execute_deploy(block_context=block_context, caller_execution_context=execution_context);
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == GET_BLOCK_HASH_SELECTOR) {
        execute_get_block_hash(block_context=block_context);
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == GET_CLASS_HASH_AT_SELECTOR) {
        execute_get_class_hash_at();
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == REPLACE_CLASS_SELECTOR) {
        execute_replace_class(contract_address=execution_context.execution_info.contract_address);
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == KECCAK_SELECTOR) {
        execute_keccak();
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SHA256_PROCESS_BLOCK_SELECTOR) {
        execute_sha256_process_block();
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256K1_GET_POINT_FROM_X_SELECTOR) {
        execute_secp256k1_get_point_from_x();
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256R1_GET_POINT_FROM_X_SELECTOR) {
        execute_secp256r1_get_point_from_x();
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256K1_NEW_SELECTOR) {
        execute_secp256k1_new();
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256R1_NEW_SELECTOR) {
        execute_secp256r1_new();
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256K1_ADD_SELECTOR) {
        execute_secp256k1_add();
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256R1_ADD_SELECTOR) {
        execute_secp256r1_add();
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256K1_MUL_SELECTOR) {
        execute_secp256k1_mul();
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256R1_MUL_SELECTOR) {
        execute_secp256r1_mul();
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256K1_GET_XY_SELECTOR) {
        execute_secp_get_xy(
            curve_prime=Uint256(low=SECP256K1_PRIME_LOW, high=SECP256K1_PRIME_HIGH),
            gas_cost=SECP256K1_GET_XY_GAS_COST,
        );
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SECP256R1_GET_XY_SELECTOR) {
        execute_secp_get_xy(
            curve_prime=Uint256(low=SECP256R1_PRIME_LOW, high=SECP256R1_PRIME_HIGH),
            gas_cost=SECP256R1_GET_XY_GAS_COST,
        );
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    if (selector == SEND_MESSAGE_TO_L1_SELECTOR) {
        execute_send_message_to_l1(
            contract_address=execution_context.execution_info.contract_address
        );
        %{ exit_syscall() %}
        return execute_syscalls(
            block_context=block_context,
            execution_context=execution_context,
            syscall_ptr_end=syscall_ptr_end,
        );
    }

    assert selector = META_TX_V0_SELECTOR;
    execute_meta_tx_v0(block_context=block_context, caller_execution_context=execution_context);
    %{ exit_syscall() %}
    return execute_syscalls(
        block_context=block_context,
        execution_context=execution_context,
        syscall_ptr_end=syscall_ptr_end,
    );
}

// Executes a syscall that calls another contract.
func execute_call_contract{
    range_check_ptr,
    syscall_ptr: felt*,
    builtin_ptrs: BuiltinPointers*,
    contract_state_changes: DictAccess*,
    contract_class_changes: DictAccess*,
    revert_log: RevertLogEntry*,
    outputs: OsCarriedOutputs*,
}(block_context: BlockContext*, caller_execution_context: ExecutionContext*) {
    let request = cast(syscall_ptr + RequestHeader.SIZE, CallContractRequest*);
    let (success, remaining_gas) = reduce_syscall_base_gas(
        specific_base_gas_cost=CALL_CONTRACT_GAS_COST, request_struct_size=CallContractRequest.SIZE
    );
    if (success == FALSE) {
        // Not enough gas to execute the syscall.
        return ();
    }

    tempvar contract_address = request.contract_address;
    let (state_entry: StateEntry*) = dict_read{dict_ptr=contract_state_changes}(
        key=contract_address
    );

    // Prepare execution context.
    tempvar calldata_start = request.calldata_start;
    tempvar caller_execution_info = caller_execution_context.execution_info;
    tempvar caller_address = caller_execution_info.contract_address;
    tempvar execution_context: ExecutionContext* = new ExecutionContext(
        entry_point_type=ENTRY_POINT_TYPE_EXTERNAL,
        class_hash=state_entry.class_hash,
        calldata_size=request.calldata_end - calldata_start,
        calldata=calldata_start,
        execution_info=new ExecutionInfo(
            block_info=caller_execution_info.block_info,
            tx_info=caller_execution_info.tx_info,
            caller_address=caller_address,
            contract_address=contract_address,
            selector=request.selector,
        ),
        deprecated_tx_info=caller_execution_context.deprecated_tx_info,
    );

    // Since we process the revert log backwards, entries before this point belong to the caller.
    assert [revert_log] = RevertLogEntry(selector=CHANGE_CONTRACT_ENTRY, value=caller_address);
    let revert_log = &revert_log[1];

    contract_call_helper(
        remaining_gas=remaining_gas,
        block_context=block_context,
        execution_context=execution_context,
    );

    // Entries before this point belong to the callee.
    assert [revert_log] = RevertLogEntry(
        selector=CHANGE_CONTRACT_ENTRY, value=request.contract_address
    );
    let revert_log = &revert_log[1];

    return ();
}

// Implements the library_call syscall.
func execute_library_call{
    range_check_ptr,
    syscall_ptr: felt*,
    builtin_ptrs: BuiltinPointers*,
    contract_state_changes: DictAccess*,
    contract_class_changes: DictAccess*,
    revert_log: RevertLogEntry*,
    outputs: OsCarriedOutputs*,
}(block_context: BlockContext*, caller_execution_context: ExecutionContext*) {
    let request = cast(syscall_ptr + RequestHeader.SIZE, LibraryCallRequest*);
    let (success, remaining_gas) = reduce_syscall_base_gas(
        specific_base_gas_cost=LIBRARY_CALL_GAS_COST, request_struct_size=LibraryCallRequest.SIZE
    );
    if (success == FALSE) {
        // Not enough gas to execute the syscall.
        return ();
    }

    // Prepare execution context.
    tempvar calldata_start = request.calldata_start;
    tempvar caller_execution_info = caller_execution_context.execution_info;
    tempvar execution_context: ExecutionContext* = new ExecutionContext(
        entry_point_type=ENTRY_POINT_TYPE_EXTERNAL,
        class_hash=request.class_hash,
        calldata_size=request.calldata_end - calldata_start,
        calldata=calldata_start,
        execution_info=new ExecutionInfo(
            block_info=caller_execution_info.block_info,
            tx_info=caller_execution_info.tx_info,
            caller_address=caller_execution_info.caller_address,
            contract_address=caller_execution_info.contract_address,
            selector=request.selector,
        ),
        deprecated_tx_info=caller_execution_context.deprecated_tx_info,
    );

    return contract_call_helper(
        remaining_gas=remaining_gas,
        block_context=block_context,
        execution_context=execution_context,
    );
}

// Executes a v0 meta transaction. Specifically, calls another contract where:
// * The signature is replaced with the given signature.
// * The caller is the OS (address 0).
// * The transaction version is replaced by 0.
// * The transaction hash is replaced by the corresponding version-0 transaction hash.
// The changes apply to the called contract and the inner contracts it calls.
func execute_meta_tx_v0{
    range_check_ptr,
    syscall_ptr: felt*,
    builtin_ptrs: BuiltinPointers*,
    contract_state_changes: DictAccess*,
    contract_class_changes: DictAccess*,
    revert_log: RevertLogEntry*,
    outputs: OsCarriedOutputs*,
}(block_context: BlockContext*, caller_execution_context: ExecutionContext*) {
    alloc_locals;

    let request = cast(syscall_ptr + RequestHeader.SIZE, MetaTxV0Request*);
    local calldata_start: felt* = request.calldata_start;
    local calldata_size = request.calldata_end - calldata_start;

    let specific_base_gas_cost = (
        META_TX_V0_GAS_COST + META_TX_V0_CALLDATA_FACTOR_GAS_COST * calldata_size
    );
    let (success, remaining_gas) = reduce_syscall_base_gas(
        specific_base_gas_cost=specific_base_gas_cost, request_struct_size=MetaTxV0Request.SIZE
    );
    if (success == FALSE) {
        // Not enough gas to execute the syscall.
        return ();
    }

    local contract_address = request.contract_address;
    local selector = request.selector;
    local caller_execution_info: ExecutionInfo* = caller_execution_context.execution_info;
    local old_tx_info: TxInfo* = caller_execution_info.tx_info;

    let (state_entry: StateEntry*) = dict_read{dict_ptr=contract_state_changes}(
        key=contract_address
    );

    // Compute the meta-transaction hash.
    let pedersen_ptr = builtin_ptrs.selectable.pedersen;
    with pedersen_ptr {
        let meta_tx_hash = compute_meta_tx_v0_hash(
            contract_address=contract_address,
            entry_point_selector=selector,
            calldata=calldata_start,
            calldata_size=calldata_size,
            chain_id=old_tx_info.chain_id,
        );
    }
    update_pedersen_in_builtin_ptrs(pedersen_ptr=pedersen_ptr);

    // Prepare execution context.
    tempvar new_tx_info = new TxInfo(
        version=0,
        account_contract_address=contract_address,
        max_fee=0,
        signature_start=request.signature_start,
        signature_end=request.signature_end,
        transaction_hash=meta_tx_hash,
        chain_id=old_tx_info.chain_id,
        nonce=0,
        resource_bounds_start=cast(0, ResourceBounds*),
        resource_bounds_end=cast(0, ResourceBounds*),
        tip=0,
        paymaster_data_start=cast(0, felt*),
        paymaster_data_end=cast(0, felt*),
        nonce_data_availability_mode=0,
        fee_data_availability_mode=0,
        account_deployment_data_start=cast(0, felt*),
        account_deployment_data_end=cast(0, felt*),
    );

    tempvar execution_context: ExecutionContext* = new ExecutionContext(
        entry_point_type=ENTRY_POINT_TYPE_EXTERNAL,
        class_hash=state_entry.class_hash,
        calldata_size=calldata_size,
        calldata=calldata_start,
        execution_info=new ExecutionInfo(
            block_info=caller_execution_info.block_info,
            tx_info=new_tx_info,
            caller_address=ORIGIN_ADDRESS,
            contract_address=contract_address,
            selector=selector,
        ),
        deprecated_tx_info=cast(nondet %{ segments.add() %}, DeprecatedTxInfo*),
    );
    fill_deprecated_tx_info(tx_info=new_tx_info, dst=execution_context.deprecated_tx_info);

    // Since we process the revert log backwards, entries before this point belong to the calling
    // contract.
    assert [revert_log] = RevertLogEntry(
        selector=CHANGE_CONTRACT_ENTRY, value=caller_execution_info.contract_address
    );
    let revert_log = &revert_log[1];

    contract_call_helper(
        remaining_gas=remaining_gas,
        block_context=block_context,
        execution_context=execution_context,
    );

    // Entries before this point belong to the callee.
    assert [revert_log] = RevertLogEntry(selector=CHANGE_CONTRACT_ENTRY, value=contract_address);
    let revert_log = &revert_log[1];

    return ();
}

// Executes the entry point and writes the corresponding response to the syscall_ptr.
// Assumes that syscall_ptr points at the response header.
func contract_call_helper{
    range_check_ptr,
    syscall_ptr: felt*,
    builtin_ptrs: BuiltinPointers*,
    contract_state_changes: DictAccess*,
    contract_class_changes: DictAccess*,
    revert_log: RevertLogEntry*,
    outputs: OsCarriedOutputs*,
}(remaining_gas: felt, block_context: BlockContext*, execution_context: ExecutionContext*) {
    with remaining_gas {
        let (is_reverted, retdata_size, retdata, _is_deprecated) = select_execute_entry_point_func(
            block_context=block_context, execution_context=execution_context
        );
    }

    if (is_reverted != 0) {
        // Append `ERROR_ENTRY_POINT_FAILED` to the retdata.
        assert retdata[retdata_size] = ERROR_ENTRY_POINT_FAILED;
        tempvar retdata_size = retdata_size + 1;
    } else {
        ap += 2;  // Align the stack to avoid revoked references.
        tempvar retdata_size = retdata_size;
    }

    let response_header = cast(syscall_ptr, ResponseHeader*);
    // Advance syscall pointer to the response body.
    let syscall_ptr = syscall_ptr + ResponseHeader.SIZE;

    // Write the response header.
    with_attr error_message("Predicted gas costs are inconsistent with the actual execution.") {
        assert [response_header] = ResponseHeader(gas=remaining_gas, failure_flag=is_reverted);
    }

    let response = cast(syscall_ptr, CallContractResponse*);
    // Advance syscall pointer to the next syscall.
    let syscall_ptr = syscall_ptr + CallContractResponse.SIZE;

    %{
        # Check that the actual return value matches the expected one.
        expected = memory.get_range(
            addr=ids.response.retdata_start,
            size=ids.response.retdata_end - ids.response.retdata_start,
        )
        actual = memory.get_range(addr=ids.retdata, size=ids.retdata_size)

        assert expected == actual, f'Return value mismatch; expected={expected}, actual={actual}.'
    %}

    // Write the response.
    relocate_segment(src_ptr=response.retdata_start, dest_ptr=retdata);
    assert [response] = CallContractResponse(
        retdata_start=retdata, retdata_end=retdata + retdata_size
    );

    return ();
}

// Deploys a contract and invokes its constructor.
func execute_deploy{
    range_check_ptr,
    syscall_ptr: felt*,
    builtin_ptrs: BuiltinPointers*,
    contract_state_changes: DictAccess*,
    contract_class_changes: DictAccess*,
    revert_log: RevertLogEntry*,
    outputs: OsCarriedOutputs*,
}(block_context: BlockContext*, caller_execution_context: ExecutionContext*) {
    alloc_locals;
    let request = cast(syscall_ptr + RequestHeader.SIZE, DeployRequest*);
    local constructor_calldata_start: felt* = request.constructor_calldata_start;
    local constructor_calldata_size = request.constructor_calldata_end - constructor_calldata_start;

    let specific_base_gas_cost = DEPLOY_GAS_COST + DEPLOY_CALLDATA_FACTOR_GAS_COST *
        constructor_calldata_size;
    let (success, remaining_gas) = reduce_syscall_base_gas(
        specific_base_gas_cost=specific_base_gas_cost, request_struct_size=DeployRequest.SIZE
    );
    if (success == FALSE) {
        // Not enough gas to execute the syscall.
        return ();
    }

    local caller_execution_info: ExecutionInfo* = caller_execution_context.execution_info;
    local caller_address = caller_execution_info.contract_address;

    // Verify deploy_from_zero is either 0 (FALSE) or 1 (TRUE).
    tempvar deploy_from_zero = request.deploy_from_zero;
    assert deploy_from_zero * (deploy_from_zero - 1) = 0;
    // Set deployer_address to 0 if request.deploy_from_zero is TRUE.
    let deployer_address = (1 - deploy_from_zero) * caller_address;

    let selectable_builtins = &builtin_ptrs.selectable;
    let hash_ptr = selectable_builtins.pedersen;
    with hash_ptr {
        let (contract_address) = get_contract_address(
            salt=request.contract_address_salt,
            class_hash=request.class_hash,
            constructor_calldata_size=constructor_calldata_size,
            constructor_calldata=constructor_calldata_start,
            deployer_address=deployer_address,
        );
    }
    tempvar builtin_ptrs = new BuiltinPointers(
        selectable=SelectableBuiltins(
            pedersen=hash_ptr,
            range_check=selectable_builtins.range_check,
            ecdsa=selectable_builtins.ecdsa,
            bitwise=selectable_builtins.bitwise,
            ec_op=selectable_builtins.ec_op,
            poseidon=selectable_builtins.poseidon,
            segment_arena=selectable_builtins.segment_arena,
            range_check96=selectable_builtins.range_check96,
            add_mod=selectable_builtins.add_mod,
            mul_mod=selectable_builtins.mul_mod,
        ),
        non_selectable=builtin_ptrs.non_selectable,
    );

    tempvar constructor_execution_context = new ExecutionContext(
        entry_point_type=ENTRY_POINT_TYPE_CONSTRUCTOR,
        class_hash=request.class_hash,
        calldata_size=constructor_calldata_size,
        calldata=constructor_calldata_start,
        execution_info=new ExecutionInfo(
            block_info=caller_execution_info.block_info,
            tx_info=caller_execution_info.tx_info,
            caller_address=caller_address,
            contract_address=contract_address,
            selector=CONSTRUCTOR_ENTRY_POINT_SELECTOR,
        ),
        deprecated_tx_info=caller_execution_context.deprecated_tx_info,
    );

    with remaining_gas {
        let (retdata_size, retdata) = deploy_contract(
            block_context=block_context, constructor_execution_context=constructor_execution_context
        );
    }

    let response_header = cast(syscall_ptr, ResponseHeader*);
    // Advance syscall pointer to the response body.
    let syscall_ptr = syscall_ptr + ResponseHeader.SIZE;

    // Write the response header.
    assert [response_header] = ResponseHeader(gas=remaining_gas, failure_flag=0);

    let response = cast(syscall_ptr, DeployResponse*);
    // Advance syscall pointer to the next syscall.
    let syscall_ptr = syscall_ptr + DeployResponse.SIZE;

    %{
        # Check that the actual return value matches the expected one.
        expected = memory.get_range(
            addr=ids.response.constructor_retdata_start,
            size=ids.response.constructor_retdata_end - ids.response.constructor_retdata_start,
        )
        actual = memory.get_range(addr=ids.retdata, size=ids.retdata_size)
        assert expected == actual, f'Return value mismatch; expected={expected}, actual={actual}.'
    %}

    // Write the response.
    relocate_segment(src_ptr=response.constructor_retdata_start, dest_ptr=retdata);
    assert [response] = DeployResponse(
        contract_address=contract_address,
        constructor_retdata_start=retdata,
        constructor_retdata_end=retdata + retdata_size,
    );

    return ();
}

// Reads the class hash of the given contract address.
func execute_get_class_hash_at{
    range_check_ptr, syscall_ptr: felt*, contract_state_changes: DictAccess*
}() {
    alloc_locals;
    let request = cast(syscall_ptr + RequestHeader.SIZE, GetClassHashAtRequest*);

    // Reduce gas.
    let success = reduce_syscall_gas_and_write_response_header(
        total_gas_cost=GET_CLASS_HASH_AT_GAS_COST, request_struct_size=GetClassHashAtRequest.SIZE
    );

    if (success == FALSE) {
        // Not enough gas to execute the syscall.
        return ();
    }

    let response = cast(syscall_ptr, GetClassHashAtResponse*);
    // Advance syscall pointer to the response body.
    let syscall_ptr = syscall_ptr + GetClassHashAtResponse.SIZE;

    // Read the state entry of the requested address.
    let (raw_state_entry) = dict_read{dict_ptr=contract_state_changes}(
        key=request.contract_address
    );
    let state_entry = cast(raw_state_entry, StateEntry*);

    // Write the response.
    assert [response] = GetClassHashAtResponse(class_hash=state_entry.class_hash);

    return ();
}

// Reads a value from the current contract's storage.
func execute_storage_read{range_check_ptr, syscall_ptr: felt*, contract_state_changes: DictAccess*}(
    contract_address: felt
) {
    alloc_locals;
    let request = cast(syscall_ptr + RequestHeader.SIZE, StorageReadRequest*);

    // Reduce gas.
    let success = reduce_syscall_gas_and_write_response_header(
        total_gas_cost=STORAGE_READ_GAS_COST, request_struct_size=StorageReadRequest.SIZE
    );
    if (success == FALSE) {
        // Not enough gas to execute the syscall.
        return ();
    }

    let response = cast(syscall_ptr, StorageReadResponse*);
    // Advance syscall pointer to the next syscall.
    let syscall_ptr = syscall_ptr + StorageReadResponse.SIZE;

    local state_entry: StateEntry*;
    %{
        # Fetch a state_entry in this hint and validate it in the update that comes next.
        ids.state_entry = __dict_manager.get_dict(ids.contract_state_changes)[ids.contract_address]
    %}

    // Update the contract's storage.
    static_assert StorageReadRequest.SIZE == 2;
    assert request.reserved = 0;
    tempvar value = response.value;
    %{
        # Make sure the value is cached (by reading it), to be used later on for the
        # commitment computation.
        value = execution_helper.storage_by_address[ids.contract_address].read(key=ids.request.key)
        assert ids.value == value, "Inconsistent storage value."
    %}
    tempvar storage_ptr = state_entry.storage_ptr;
    assert [storage_ptr] = DictAccess(key=request.key, prev_value=value, new_value=value);
    let storage_ptr = storage_ptr + DictAccess.SIZE;

    // Update the state.
    dict_update{dict_ptr=contract_state_changes}(
        key=contract_address,
        prev_value=cast(state_entry, felt),
        new_value=cast(
            new StateEntry(
                class_hash=state_entry.class_hash, storage_ptr=storage_ptr, nonce=state_entry.nonce
            ),
            felt,
        ),
    );

    return ();
}

// Writes a value to the current contract's storage.
func execute_storage_write{
    range_check_ptr,
    syscall_ptr: felt*,
    contract_state_changes: DictAccess*,
    revert_log: RevertLogEntry*,
}(contract_address: felt) {
    alloc_locals;
    let request = cast(syscall_ptr + RequestHeader.SIZE, StorageWriteRequest*);

    // Reduce gas.
    let success = reduce_syscall_gas_and_write_response_header(
        total_gas_cost=STORAGE_WRITE_GAS_COST, request_struct_size=StorageWriteRequest.SIZE
    );
    if (success == FALSE) {
        // Not enough gas to execute the syscall.
        return ();
    }

    local prev_value: felt;
    local state_entry: StateEntry*;
    %{
        storage = execution_helper.storage_by_address[ids.contract_address]
        ids.prev_value = storage.read(key=ids.request.key)
        storage.write(key=ids.request.key, value=ids.request.value)

        # Fetch a state_entry in this hint and validate it in the update that comes next.
        ids.state_entry = __dict_manager.get_dict(ids.contract_state_changes)[ids.contract_address]
    %}

    // Update the contract's storage.
    static_assert StorageWriteRequest.SIZE == 3;
    assert request.reserved = 0;
    tempvar storage_ptr = state_entry.storage_ptr;
    tempvar storage_key = request.key;
    assert [storage_ptr] = DictAccess(
        key=storage_key, prev_value=prev_value, new_value=request.value
    );
    let storage_ptr = storage_ptr + DictAccess.SIZE;

    assert [revert_log] = RevertLogEntry(selector=storage_key, value=prev_value);
    let revert_log = &revert_log[1];

    // Update the state.
    dict_update{dict_ptr=contract_state_changes}(
        key=contract_address,
        prev_value=cast(state_entry, felt),
        new_value=cast(
            new StateEntry(
                class_hash=state_entry.class_hash, storage_ptr=storage_ptr, nonce=state_entry.nonce
            ),
            felt,
        ),
    );

    return ();
}

// Gets the block hash of the block at given block number.
func execute_get_block_hash{
    range_check_ptr, syscall_ptr: felt*, contract_state_changes: DictAccess*
}(block_context: BlockContext*) {
    alloc_locals;
    let request = cast(syscall_ptr + RequestHeader.SIZE, GetBlockHashRequest*);

    // Reduce gas.
    let (success, remaining_gas) = reduce_syscall_base_gas(
        specific_base_gas_cost=GET_BLOCK_HASH_GAS_COST, request_struct_size=GetBlockHashRequest.SIZE
    );
    if (success == FALSE) {
        // Not enough gas to execute the syscall; in that case, 'reduce_syscall_base_gas' already
        // wrote the response objects and advanced the syscall pointer.
        return ();
    }

    // Handle out of range block number.
    let request_block_number = request.block_number;
    let current_block_number = block_context.block_info_for_execute.block_number;

    // A block number is a u64. STORED_BLOCK_HASH_BUFFER is 10.
    // The following computations will not overflow.
    if (nondet %{
            ids.request_block_number > \
                       ids.current_block_number - ids.STORED_BLOCK_HASH_BUFFER
        %} != FALSE) {
        assert_lt(current_block_number, request_block_number + STORED_BLOCK_HASH_BUFFER);
        write_failure_response(
            remaining_gas=remaining_gas, failure_felt=ERROR_BLOCK_NUMBER_OUT_OF_RANGE
        );
        return ();
    }

    assert_le(request_block_number + STORED_BLOCK_HASH_BUFFER, current_block_number);

    // Gas reduction has succeeded and the request is valid; write the response header.
    let response_header = cast(syscall_ptr, ResponseHeader*);
    // Advance syscall pointer to the response body.
    let syscall_ptr = syscall_ptr + ResponseHeader.SIZE;
    assert [response_header] = ResponseHeader(gas=remaining_gas, failure_flag=0);

    let response = cast(syscall_ptr, GetBlockHashResponse*);
    // Advance syscall pointer to the next syscall.
    let syscall_ptr = syscall_ptr + GetBlockHashResponse.SIZE;

    // Fetch the block hash contract state.
    local state_entry: StateEntry*;
    %{
        # Fetch a state_entry in this hint. Validate it in the update that comes next.
        ids.state_entry = __dict_manager.get_dict(ids.contract_state_changes)[
            ids.BLOCK_HASH_CONTRACT_ADDRESS]
    %}

    // Read from storage.
    tempvar block_hash = response.block_hash;
    tempvar storage_ptr = state_entry.storage_ptr;
    assert [storage_ptr] = DictAccess(
        key=request_block_number, prev_value=block_hash, new_value=block_hash
    );
    let storage_ptr = storage_ptr + DictAccess.SIZE;

    // Update the state.
    dict_update{dict_ptr=contract_state_changes}(
        key=BLOCK_HASH_CONTRACT_ADDRESS,
        prev_value=cast(state_entry, felt),
        new_value=cast(
            new StateEntry(
                class_hash=state_entry.class_hash, storage_ptr=storage_ptr, nonce=state_entry.nonce
            ),
            felt,
        ),
    );

    return ();
}

// Gets the execution info.
func execute_get_execution_info{range_check_ptr, syscall_ptr: felt*}(
    execution_context: ExecutionContext*
) {
    alloc_locals;

    // Reduce gas.
    let success = reduce_syscall_gas_and_write_response_header(
        total_gas_cost=GET_EXECUTION_INFO_GAS_COST, request_struct_size=0
    );
    if (success == FALSE) {
        // Not enough gas to execute the syscall.
        return ();
    }

    let response = cast(syscall_ptr, GetExecutionInfoResponse*);
    // Advance syscall pointer to the next syscall.
    let syscall_ptr = syscall_ptr + GetExecutionInfoResponse.SIZE;

    local execution_info: ExecutionInfo* = execution_context.execution_info;
    local tx_info: TxInfo* = execution_info.tx_info;
    let exclude_l1_data_gas = should_exclude_l1_data_gas(execution_context.class_hash);
    let v1_bound = is_v1_bound_account_cairo1(execution_context.class_hash);
    let check_tip = check_tip_for_v1_bound_accounts(tx_info.tip);
    if (tx_info.version == 3 and v1_bound != FALSE and check_tip != FALSE) {
        assert exclude_l1_data_gas = FALSE;

        tempvar response_tx_info = response.execution_info.tx_info;

        assert [response_tx_info] = TxInfo(
            version=1,
            account_contract_address=tx_info.account_contract_address,
            max_fee=tx_info.max_fee,
            signature_start=tx_info.signature_start,
            signature_end=tx_info.signature_end,
            transaction_hash=tx_info.transaction_hash,
            chain_id=tx_info.chain_id,
            nonce=tx_info.nonce,
            resource_bounds_start=tx_info.resource_bounds_start,
            resource_bounds_end=tx_info.resource_bounds_end,
            tip=tx_info.tip,
            paymaster_data_start=tx_info.paymaster_data_start,
            paymaster_data_end=tx_info.paymaster_data_end,
            nonce_data_availability_mode=tx_info.nonce_data_availability_mode,
            fee_data_availability_mode=tx_info.fee_data_availability_mode,
            account_deployment_data_start=tx_info.account_deployment_data_start,
            account_deployment_data_end=tx_info.account_deployment_data_end,
        );

        static_assert GetExecutionInfoResponse.SIZE == 1;
        assert [response.execution_info] = ExecutionInfo(
            block_info=execution_info.block_info,
            tx_info=response_tx_info,
            caller_address=execution_info.caller_address,
            contract_address=execution_info.contract_address,
            selector=execution_info.selector,
        );
        return ();
    }

    if (tx_info.version == 3 and exclude_l1_data_gas != FALSE) {
        tempvar response_tx_info = response.execution_info.tx_info;
        let (excluded_resource_bounds_end) = exclude_data_gas_of_resource_bounds(
            resource_bounds_start=tx_info.resource_bounds_start,
            resource_bounds_end=tx_info.resource_bounds_end,
        );

        assert [response_tx_info] = TxInfo(
            version=tx_info.version,
            account_contract_address=tx_info.account_contract_address,
            max_fee=tx_info.max_fee,
            signature_start=tx_info.signature_start,
            signature_end=tx_info.signature_end,
            transaction_hash=tx_info.transaction_hash,
            chain_id=tx_info.chain_id,
            nonce=tx_info.nonce,
            resource_bounds_start=tx_info.resource_bounds_start,
            resource_bounds_end=excluded_resource_bounds_end,
            tip=tx_info.tip,
            paymaster_data_start=tx_info.paymaster_data_start,
            paymaster_data_end=tx_info.paymaster_data_end,
            nonce_data_availability_mode=tx_info.nonce_data_availability_mode,
            fee_data_availability_mode=tx_info.fee_data_availability_mode,
            account_deployment_data_start=tx_info.account_deployment_data_start,
            account_deployment_data_end=tx_info.account_deployment_data_end,
        );

        static_assert GetExecutionInfoResponse.SIZE == 1;
        assert [response.execution_info] = ExecutionInfo(
            block_info=execution_info.block_info,
            tx_info=response_tx_info,
            caller_address=execution_info.caller_address,
            contract_address=execution_info.contract_address,
            selector=execution_info.selector,
        );
        return ();
    }

    assert [response] = GetExecutionInfoResponse(execution_info=execution_info);
    return ();
}

// Replaces the class.
func execute_replace_class{
    range_check_ptr,
    syscall_ptr: felt*,
    contract_state_changes: DictAccess*,
    revert_log: RevertLogEntry*,
}(contract_address: felt) {
    alloc_locals;
    let request = cast(syscall_ptr + RequestHeader.SIZE, ReplaceClassRequest*);

    // Reduce gas.
    let success = reduce_syscall_gas_and_write_response_header(
        total_gas_cost=REPLACE_CLASS_GAS_COST, request_struct_size=ReplaceClassRequest.SIZE
    );
    if (success == FALSE) {
        // Not enough gas to execute the syscall.
        return ();
    }

    let class_hash = request.class_hash;

    local state_entry: StateEntry*;
    %{
        # Fetch a state_entry in this hint and validate it in the update at the end
        # of this function.
        ids.state_entry = __dict_manager.get_dict(ids.contract_state_changes)[ids.contract_address]
    %}

    tempvar new_state_entry = new StateEntry(
        class_hash=class_hash, storage_ptr=state_entry.storage_ptr, nonce=state_entry.nonce
    );

    dict_update{dict_ptr=contract_state_changes}(
        key=contract_address,
        prev_value=cast(state_entry, felt),
        new_value=cast(new_state_entry, felt),
    );

    assert [revert_log] = RevertLogEntry(selector=CHANGE_CLASS_ENTRY, value=state_entry.class_hash);
    let revert_log = &revert_log[1];

    return ();
}

// Executes the keccak system call.
func execute_keccak{
    range_check_ptr, builtin_ptrs: BuiltinPointers*, syscall_ptr: felt*, outputs: OsCarriedOutputs*
}() {
    alloc_locals;
    let request_header = cast(syscall_ptr, RequestHeader*);
    let request = cast(syscall_ptr + RequestHeader.SIZE, KeccakRequest*);
    tempvar input_start = request.input_start;
    tempvar input_end = request.input_end;
    let len = input_end - input_start;
    let (local q, r) = unsigned_div_rem(len, KECCAK_FULL_RATE_IN_WORDS);

    // Note that if KECCAK_GAS_COST > SYSCALL_BASE_GAS_COST, we need to call
    // `reduce_syscall_base_gas` before the 'if' bellow to be consistent with the Sequencer.
    static_assert KECCAK_GAS_COST == SYSCALL_BASE_GAS_COST;
    if (r != 0) {
        let syscall_ptr = syscall_ptr + RequestHeader.SIZE + KeccakRequest.SIZE;
        write_failure_response(
            remaining_gas=request_header.gas, failure_felt=ERROR_INVALID_INPUT_LEN
        );
        return ();
    }

    let required_gas = KECCAK_GAS_COST + q * KECCAK_ROUND_COST_GAS_COST;
    let success = reduce_syscall_gas_and_write_response_header(
        total_gas_cost=required_gas, request_struct_size=KeccakRequest.SIZE
    );

    if (success == FALSE) {
        // Not enough gas to execute the syscall.
        return ();
    }

    let selectable_builtins = &builtin_ptrs.selectable;
    let non_selectable_builtins = &builtin_ptrs.non_selectable;
    let bitwise_ptr = selectable_builtins.bitwise;
    let keccak_ptr = non_selectable_builtins.keccak;
    with bitwise_ptr, keccak_ptr {
        let (res) = keccak_padded_input(inputs=input_start, n_blocks=q);
    }

    assert [cast(syscall_ptr, KeccakResponse*)] = KeccakResponse(
        result_low=res.low, result_high=res.high
    );
    let syscall_ptr = syscall_ptr + KeccakResponse.SIZE;

    tempvar builtin_ptrs = new BuiltinPointers(
        selectable=SelectableBuiltins(
            pedersen=selectable_builtins.pedersen,
            range_check=selectable_builtins.range_check,
            ecdsa=selectable_builtins.ecdsa,
            bitwise=bitwise_ptr,
            ec_op=selectable_builtins.ec_op,
            poseidon=selectable_builtins.poseidon,
            segment_arena=selectable_builtins.segment_arena,
            range_check96=selectable_builtins.range_check96,
            add_mod=selectable_builtins.add_mod,
            mul_mod=selectable_builtins.mul_mod,
        ),
        non_selectable=NonSelectableBuiltins(
            keccak=keccak_ptr, sha256=non_selectable_builtins.sha256
        ),
    );
    return ();
}

// Executes the sha256_process_block system call.
func execute_sha256_process_block{
    range_check_ptr, builtin_ptrs: BuiltinPointers*, syscall_ptr: felt*, outputs: OsCarriedOutputs*
}() {
    alloc_locals;

    let request = cast(syscall_ptr + RequestHeader.SIZE, Sha256ProcessBlockRequest*);

    // Reduce gas.
    let success = reduce_syscall_gas_and_write_response_header(
        total_gas_cost=SHA256_PROCESS_BLOCK_GAS_COST,
        request_struct_size=Sha256ProcessBlockRequest.SIZE,
    );
    if (success == 0) {
        // Not enough gas to execute the syscall.
        return ();
    }

    local sha256_ptr: Sha256ProcessBlock* = builtin_ptrs.non_selectable.sha256;

    let input: Sha256Input* = &sha256_ptr.input;
    assert [input] = [request.input_start];

    let state: Sha256State* = &sha256_ptr.in_state;
    assert [state] = [request.state_ptr];

    let res: Sha256State* = &sha256_ptr.out_state;
    let sha256_ptr = &sha256_ptr[1];

    assert [cast(syscall_ptr, Sha256ProcessBlockResponse*)] = Sha256ProcessBlockResponse(
        state_ptr=res
    );
    let syscall_ptr = syscall_ptr + Sha256ProcessBlockResponse.SIZE;

    tempvar builtin_ptrs = new BuiltinPointers(
        selectable=builtin_ptrs.selectable,
        non_selectable=NonSelectableBuiltins(
            keccak=builtin_ptrs.non_selectable.keccak, sha256=sha256_ptr
        ),
    );
    return ();
}

// Executes the secp256k1_add system call.
func execute_secp256k1_add{range_check_ptr, syscall_ptr: felt*}() {
    alloc_locals;
    let request = cast(syscall_ptr + RequestHeader.SIZE, Secp256k1AddRequest*);

    // Reduce gas.
    let success = reduce_syscall_gas_and_write_response_header(
        total_gas_cost=SECP256K1_ADD_GAS_COST, request_struct_size=Secp256k1AddRequest.SIZE
    );
    if (success == 0) {
        // Not enough gas to execute the syscall.
        return ();
    }

    let (res) = secp256k1_ec_add(point0=[request.p0], point1=[request.p1]);
    let response = cast(syscall_ptr, Secp256k1AddResponse*);
    static_assert Secp256k1AddResponse.SIZE == 1;
    assert [response.ec_point] = res;
    let syscall_ptr = syscall_ptr + Secp256k1AddResponse.SIZE;
    return ();
}

// Executes the secp256r1_add system call.
func execute_secp256r1_add{range_check_ptr, syscall_ptr: felt*}() {
    alloc_locals;
    let request = cast(syscall_ptr + RequestHeader.SIZE, Secp256r1AddRequest*);

    // Reduce gas.
    let success = reduce_syscall_gas_and_write_response_header(
        total_gas_cost=SECP256R1_ADD_GAS_COST, request_struct_size=Secp256r1AddRequest.SIZE
    );
    if (success == 0) {
        // Not enough gas to execute the syscall.
        return ();
    }

    let (res) = secp256r1_ec_add(point0=[request.p0], point1=[request.p1]);
    let response = cast(syscall_ptr, Secp256r1AddResponse*);
    static_assert Secp256r1AddResponse.SIZE == 1;
    assert [response.ec_point] = res;
    let syscall_ptr = syscall_ptr + Secp256r1AddResponse.SIZE;
    return ();
}

// Executes the secp256k1_mul system call.
func execute_secp256k1_mul{range_check_ptr, syscall_ptr: felt*}() {
    alloc_locals;
    let request = cast(syscall_ptr + RequestHeader.SIZE, Secp256k1MulRequest*);

    // Reduce gas.
    let success = reduce_syscall_gas_and_write_response_header(
        total_gas_cost=SECP256K1_MUL_GAS_COST, request_struct_size=Secp256k1MulRequest.SIZE
    );
    if (success == 0) {
        // Not enough gas to execute the syscall.
        return ();
    }

    let (res) = secp256k1_ec_mul_by_uint256(point=[request.p], scalar=request.scalar);
    let response = cast(syscall_ptr, Secp256k1MulResponse*);
    static_assert Secp256k1MulResponse.SIZE == 1;
    assert [response.ec_point] = res;
    let syscall_ptr = syscall_ptr + Secp256k1MulResponse.SIZE;

    return ();
}

// Executes the secp256r1_mul system call.
func execute_secp256r1_mul{range_check_ptr, syscall_ptr: felt*}() {
    alloc_locals;
    let request = cast(syscall_ptr + RequestHeader.SIZE, Secp256r1MulRequest*);

    // Reduce gas.
    let success = reduce_syscall_gas_and_write_response_header(
        total_gas_cost=SECP256R1_MUL_GAS_COST, request_struct_size=Secp256r1MulRequest.SIZE
    );
    if (success == 0) {
        // Not enough gas to execute the syscall.
        return ();
    }

    let (res) = secp256r1_ec_mul_by_uint256(point=[request.p], scalar=request.scalar);
    let response = cast(syscall_ptr, Secp256r1MulResponse*);
    static_assert Secp256r1MulResponse.SIZE == 1;
    assert [response.ec_point] = res;
    let syscall_ptr = syscall_ptr + Secp256r1MulResponse.SIZE;

    return ();
}

// Executes the secp256k1_new system call.
func execute_secp256k1_new{range_check_ptr, syscall_ptr: felt*}() {
    alloc_locals;
    let request = cast(syscall_ptr + RequestHeader.SIZE, Secp256k1NewRequest*);

    let (success, remaining_gas) = reduce_syscall_base_gas(
        specific_base_gas_cost=SECP256K1_NEW_GAS_COST, request_struct_size=Secp256k1NewRequest.SIZE
    );
    if (success == 0) {
        // Not enough gas to execute the syscall.
        return ();
    }

    let secp_prime = Uint256(low=SECP256K1_PRIME_LOW, high=SECP256K1_PRIME_HIGH);
    let (is_x_valid) = uint256_lt(request.x, secp_prime);
    let (is_y_valid) = uint256_lt(request.y, secp_prime);

    if ((is_x_valid + is_y_valid) != 2) {
        write_failure_response(remaining_gas=remaining_gas, failure_felt=ERROR_INVALID_ARGUMENT);
        return ();
    }

    let response_header = cast(syscall_ptr, ResponseHeader*);
    assert [response_header] = ResponseHeader(gas=remaining_gas, failure_flag=0);
    let syscall_ptr = syscall_ptr + ResponseHeader.SIZE;

    let response = cast(syscall_ptr, Secp256k1NewResponse*);
    let syscall_ptr = syscall_ptr + Secp256k1NewResponse.SIZE;
    if (request.x.low == 0 and request.x.high == 0 and request.y.low == 0 and request.y.high == 0) {
        // Return the point at infinity.
        assert response.not_on_curve = 0;
        assert [response.ec_point] = SecpPoint(x=BigInt3(0, 0, 0), y=BigInt3(0, 0, 0));
        return ();
    }

    let (x) = uint256_to_bigint(request.x);

    let not_on_curve = response.not_on_curve;
    tempvar result_ptr = cast(
        nondet %{ ids.response.ec_point.address_ if ids.not_on_curve == 0 else segments.add() %},
        SecpPoint*,
    );

    // Note that there are no constraints on response.ec_point in the failure case.

    let (is_on_curve) = secp256k1_try_get_point_from_x(x=x, v=request.y.low, result=result_ptr);
    if (is_on_curve == 0) {
        // Return early to avoid dealing with range_check_ptr divergence.
        assert response.not_on_curve = 1;
        return ();
    }

    let (point_y) = bigint_to_uint256(result_ptr.y);
    if (point_y.low == request.y.low and point_y.high == request.y.high) {
        assert [response] = Secp256k1NewResponse(not_on_curve=0, ec_point=result_ptr);
        return ();
    }
    assert response.not_on_curve = 1;
    return ();
}

// Executes the secp256r1_new system call.
func execute_secp256r1_new{range_check_ptr, syscall_ptr: felt*}() {
    alloc_locals;
    let request = cast(syscall_ptr + RequestHeader.SIZE, Secp256r1NewRequest*);

    let (success, remaining_gas) = reduce_syscall_base_gas(
        specific_base_gas_cost=SECP256R1_NEW_GAS_COST, request_struct_size=Secp256r1NewRequest.SIZE
    );
    if (success == 0) {
        // Not enough gas to execute the syscall.
        return ();
    }

    let secp_prime = Uint256(low=SECP256R1_PRIME_LOW, high=SECP256R1_PRIME_HIGH);
    let (is_x_valid) = uint256_lt(request.x, secp_prime);
    let (is_y_valid) = uint256_lt(request.y, secp_prime);

    if ((is_x_valid + is_y_valid) != 2) {
        write_failure_response(remaining_gas=remaining_gas, failure_felt=ERROR_INVALID_ARGUMENT);
        return ();
    }

    let response_header = cast(syscall_ptr, ResponseHeader*);
    assert [response_header] = ResponseHeader(gas=remaining_gas, failure_flag=0);
    let syscall_ptr = syscall_ptr + ResponseHeader.SIZE;

    let response = cast(syscall_ptr, Secp256r1NewResponse*);
    let syscall_ptr = syscall_ptr + Secp256r1NewResponse.SIZE;
    if (request.x.low == 0 and request.x.high == 0 and request.y.low == 0 and request.y.high == 0) {
        // Return the point at infinity.
        assert response.not_on_curve = 0;
        assert [response.ec_point] = SecpPoint(x=BigInt3(0, 0, 0), y=BigInt3(0, 0, 0));
        return ();
    }

    let (x) = uint256_to_bigint(request.x);

    let not_on_curve = response.not_on_curve;
    tempvar result_ptr = cast(
        nondet %{ ids.response.ec_point.address_ if ids.not_on_curve == 0 else segments.add() %},
        SecpPoint*,
    );

    // Note that there are no constraints on response.ec_point in the failure case.

    let (is_on_curve) = secp256r1_try_get_point_from_x(x=x, v=request.y.low, result=result_ptr);
    if (is_on_curve == 0) {
        // Return early to avoid dealing with range_check_ptr divergence.
        assert response.not_on_curve = 1;
        return ();
    }

    let (point_y) = bigint_to_uint256(result_ptr.y);
    if (point_y.low == request.y.low and point_y.high == request.y.high) {
        assert [response] = Secp256r1NewResponse(not_on_curve=0, ec_point=result_ptr);
        return ();
    }
    assert response.not_on_curve = 1;
    return ();
}

// Executes the secp256k1_get_point_from_x system call.
func execute_secp256k1_get_point_from_x{range_check_ptr, syscall_ptr: felt*}() {
    alloc_locals;
    let request = cast(syscall_ptr + RequestHeader.SIZE, Secp256k1GetPointFromXRequest*);
    let (success, remaining_gas) = reduce_syscall_base_gas(
        specific_base_gas_cost=SECP256K1_GET_POINT_FROM_X_GAS_COST,
        request_struct_size=Secp256k1GetPointFromXRequest.SIZE,
    );
    if (success == 0) {
        // Not enough gas to execute the syscall.
        return ();
    }

    let secp_prime = Uint256(low=SECP256K1_PRIME_LOW, high=SECP256K1_PRIME_HIGH);
    let (is_x_valid) = uint256_lt(request.x, secp_prime);

    if (is_x_valid == 0) {
        write_failure_response(remaining_gas=remaining_gas, failure_felt=ERROR_INVALID_ARGUMENT);
        return ();
    }

    let response_header = cast(syscall_ptr, ResponseHeader*);
    assert [response_header] = ResponseHeader(gas=remaining_gas, failure_flag=0);
    let syscall_ptr = syscall_ptr + ResponseHeader.SIZE;

    let response = cast(syscall_ptr, Secp256k1NewResponse*);
    let syscall_ptr = syscall_ptr + Secp256k1NewResponse.SIZE;

    let (x) = uint256_to_bigint(request.x);
    // Note that there are no constraints on response.ec_point in the failure case.
    let (is_on_curve) = secp256k1_try_get_point_from_x(
        x=x, v=request.y_parity, result=response.ec_point
    );
    assert response.not_on_curve = 1 - is_on_curve;

    return ();
}

// Executes the secp256r1_get_point_from_x system call.
func execute_secp256r1_get_point_from_x{range_check_ptr, syscall_ptr: felt*}() {
    alloc_locals;
    let request = cast(syscall_ptr + RequestHeader.SIZE, Secp256r1GetPointFromXRequest*);
    let (success, remaining_gas) = reduce_syscall_base_gas(
        specific_base_gas_cost=SECP256R1_GET_POINT_FROM_X_GAS_COST,
        request_struct_size=Secp256r1GetPointFromXRequest.SIZE,
    );
    if (success == 0) {
        // Not enough gas to execute the syscall.
        return ();
    }

    let secp_prime = Uint256(low=SECP256R1_PRIME_LOW, high=SECP256R1_PRIME_HIGH);
    let (is_x_valid) = uint256_lt(request.x, secp_prime);

    if (is_x_valid == 0) {
        write_failure_response(remaining_gas=remaining_gas, failure_felt=ERROR_INVALID_ARGUMENT);
        return ();
    }

    let response_header = cast(syscall_ptr, ResponseHeader*);
    assert [response_header] = ResponseHeader(gas=remaining_gas, failure_flag=0);
    let syscall_ptr = syscall_ptr + ResponseHeader.SIZE;

    let response = cast(syscall_ptr, Secp256r1NewResponse*);
    let syscall_ptr = syscall_ptr + Secp256r1NewResponse.SIZE;

    let (x) = uint256_to_bigint(request.x);
    // Note that there are no constraints on response.ec_point in the failure case.
    let (is_on_curve) = secp256r1_try_get_point_from_x(
        x=x, v=request.y_parity, result=response.ec_point
    );
    assert response.not_on_curve = 1 - is_on_curve;

    return ();
}

// Executes the secp256k1_get_xy system call.
// Takes the curve prime and the gas cost of the syscall as arguments.
func execute_secp_get_xy{range_check_ptr, syscall_ptr: felt*}(
    curve_prime: Uint256, gas_cost: felt
) {
    alloc_locals;
    let request = cast(syscall_ptr + RequestHeader.SIZE, SecpGetXyRequest*);

    // Reduce gas.
    let success = reduce_syscall_gas_and_write_response_header(
        total_gas_cost=gas_cost, request_struct_size=SecpGetXyRequest.SIZE
    );
    if (success == 0) {
        // Not enough gas to execute the syscall.
        return ();
    }

    tempvar ec_point = request.ec_point;
    let (x) = bigint_to_uint256(ec_point.x);
    let (y) = bigint_to_uint256(ec_point.y);

    assert_uint256_lt(x, curve_prime);
    assert_uint256_lt(y, curve_prime);

    assert [cast(syscall_ptr, SecpGetXyResponse*)] = SecpGetXyResponse(x=x, y=y);

    let syscall_ptr = syscall_ptr + SecpGetXyResponse.SIZE;
    return ();
}

// Sends a message to L1.
func execute_send_message_to_l1{range_check_ptr, syscall_ptr: felt*, outputs: OsCarriedOutputs*}(
    contract_address: felt
) {
    alloc_locals;
    let request = cast(syscall_ptr + RequestHeader.SIZE, SendMessageToL1Request*);
    let success = reduce_syscall_gas_and_write_response_header(
        total_gas_cost=SEND_MESSAGE_TO_L1_GAS_COST, request_struct_size=SendMessageToL1Request.SIZE
    );
    if (success == FALSE) {
        // Not enough gas to execute the syscall.
        return ();
    }

    tempvar payload_start = request.payload_start;
    tempvar payload_size = request.payload_end - payload_start;

    assert [outputs.messages_to_l1] = MessageToL1Header(
        from_address=contract_address, to_address=request.to_address, payload_size=payload_size
    );
    memcpy(
        dst=outputs.messages_to_l1 + MessageToL1Header.SIZE, src=payload_start, len=payload_size
    );
    let (outputs) = os_carried_outputs_new(
        messages_to_l1=outputs.messages_to_l1 + MessageToL1Header.SIZE + payload_size,
        messages_to_l2=outputs.messages_to_l2,
    );

    return ();
}

// Reduces the total amount of gas required for the current syscall and writes the response header.
// In case of out-of-gas failure, writes the FailureReason object to syscall_ptr.
// Returns 1 if the gas reduction succeeded and 0 otherwise.
func reduce_syscall_gas_and_write_response_header{range_check_ptr, syscall_ptr: felt*}(
    total_gas_cost: felt, request_struct_size: felt
) -> felt {
    let (success, remaining_gas) = reduce_syscall_base_gas(
        specific_base_gas_cost=total_gas_cost, request_struct_size=request_struct_size
    );
    if (success != FALSE) {
        // Reduction has succeeded; write the response header.
        let response_header = cast(syscall_ptr, ResponseHeader*);
        // Advance syscall pointer to the response body.
        let syscall_ptr = syscall_ptr + ResponseHeader.SIZE;
        assert [response_header] = ResponseHeader(gas=remaining_gas, failure_flag=0);

        return 1;
    }

    // Reduction has failed; in that case, 'reduce_syscall_base_gas' already wrote the response
    // objects and advanced the syscall pointer.
    return 0;
}

// Returns a failure response with a single felt.
@known_ap_change
func write_failure_response{syscall_ptr: felt*}(remaining_gas: felt, failure_felt: felt) {
    let response_header = cast(syscall_ptr, ResponseHeader*);
    // Advance syscall pointer to the response body.
    let syscall_ptr = syscall_ptr + ResponseHeader.SIZE;

    // Write the response header.
    assert [response_header] = ResponseHeader(gas=remaining_gas, failure_flag=1);

    let failure_reason: FailureReason* = cast(syscall_ptr, FailureReason*);
    // Advance syscall pointer to the next syscall.
    let syscall_ptr = syscall_ptr + FailureReason.SIZE;

    // Write the failure reason.
    tempvar start = failure_reason.start;
    assert start[0] = failure_felt;
    assert failure_reason.end = start + 1;
    return ();
}

// Reduces the base amount of gas for the current syscall.
// In case of out-of-gas failure, writes the corresponding ResponseHeader and FailureReason
// objects to syscall_ptr.
// Returns 1 if the gas reduction succeeded and 0 otherwise, along with the remaining gas.
func reduce_syscall_base_gas{range_check_ptr, syscall_ptr: felt*}(
    specific_base_gas_cost: felt, request_struct_size: felt
) -> (success: felt, remaining_gas: felt) {
    let request_header = cast(syscall_ptr, RequestHeader*);
    // Advance syscall pointer to the response header.
    let syscall_ptr = syscall_ptr + RequestHeader.SIZE + request_struct_size;

    // Refund the pre-charged base gas.
    let required_gas = specific_base_gas_cost - SYSCALL_BASE_GAS_COST;
    tempvar initial_gas = request_header.gas;
    if (nondet %{ ids.initial_gas >= ids.required_gas %} != FALSE) {
        tempvar remaining_gas = initial_gas - required_gas;
        assert_nn(remaining_gas);
        return (success=1, remaining_gas=remaining_gas);
    }

    // Handle out-of-gas.
    assert_lt(initial_gas, required_gas);
    write_failure_response(remaining_gas=initial_gas, failure_felt=ERROR_OUT_OF_GAS);

    return (success=0, remaining_gas=initial_gas);
}
