from starkware.cairo.common.alloc import alloc
from starkware.cairo.common.builtin_poseidon.poseidon import poseidon_hash_many
from starkware.cairo.common.cairo_builtins import HashBuiltin, PoseidonBuiltin
from starkware.cairo.common.hash_state import (
    hash_finalize,
    hash_init,
    hash_update,
    hash_update_single,
    hash_update_with_hashchain,
)
from starkware.cairo.common.hash_state_poseidon import HashState as PoseidonHashState
from starkware.cairo.common.hash_state_poseidon import hash_finalize as poseidon_hash_finalize
from starkware.cairo.common.hash_state_poseidon import hash_init as poseidon_hash_init
from starkware.cairo.common.hash_state_poseidon import hash_update as poseidon_hash_update
from starkware.cairo.common.hash_state_poseidon import (
    hash_update_single as poseidon_hash_update_single,
)
from starkware.cairo.common.hash_state_poseidon import (
    hash_update_with_nested_hash as poseidon_hash_update_with_nested_hash,
)
from starkware.cairo.common.math import assert_nn, assert_nn_le, assert_not_zero
from starkware.cairo.common.registers import get_fp_and_pc
from starkware.starknet.common.new_syscalls import ResourceBounds
from starkware.starknet.core.os.builtins import BuiltinPointers, SelectableBuiltins
from starkware.starknet.core.os.constants import (
    CONSTRUCTOR_ENTRY_POINT_SELECTOR,
    DEPLOY_HASH_PREFIX,
    EXECUTE_ENTRY_POINT_SELECTOR,
    INVOKE_HASH_PREFIX,
    L1_DATA_GAS,
    L1_DATA_GAS_INDEX,
    L1_GAS,
    L1_GAS_INDEX,
    L1_HANDLER_HASH_PREFIX,
    L1_HANDLER_VERSION,
    L2_GAS,
    L2_GAS_INDEX,
)
from starkware.starknet.core.os.execution.execute_entry_point import ExecutionContext

// Common fields of an account transaction. Used in hash calculation.
struct CommonTxFields {
    // The prefix of the transaction hash.
    tx_hash_prefix: felt,
    // The version of the transaction.
    version: felt,
    // The address of the account contract that sent the transaction.
    sender_address: felt,
    // The chain id.
    chain_id: felt,
    // The nonce of the transaction: a sequential transaction number, attached to the account
    // contract. Allows transaction ordering and prevents re-execution of transactions.
    nonce: felt,
    // Fee-related fields.
    // The tip.
    tip: felt,
    n_resource_bounds: felt,
    // An array of ResourceBounds structs.
    resource_bounds: ResourceBounds*,
    // If specified, the paymaster should pay for the execution of the tx.
    // The data includes the address of the paymaster sponsoring the transaction, followed by extra
    // data to send to the paymaster.
    paymaster_data_length: felt,
    paymaster_data: felt*,
    // The data availability mode for the nonce.
    nonce_data_availability_mode: felt,
    // The data availability mode for the account balance from which fee will be taken.
    fee_data_availability_mode: felt,
}

func deprecated_get_transaction_hash{hash_ptr: HashBuiltin*}(
    tx_hash_prefix: felt,
    version: felt,
    contract_address: felt,
    entry_point_selector: felt,
    calldata_size: felt,
    calldata: felt*,
    max_fee: felt,
    chain_id: felt,
    additional_data_size: felt,
    additional_data: felt*,
) -> (tx_hash: felt) {
    let (hash_state_ptr) = hash_init();
    let (hash_state_ptr) = hash_update_single(hash_state_ptr=hash_state_ptr, item=tx_hash_prefix);
    let (hash_state_ptr) = hash_update_single(hash_state_ptr=hash_state_ptr, item=version);
    let (hash_state_ptr) = hash_update_single(hash_state_ptr=hash_state_ptr, item=contract_address);
    let (hash_state_ptr) = hash_update_single(
        hash_state_ptr=hash_state_ptr, item=entry_point_selector
    );
    let (hash_state_ptr) = hash_update_with_hashchain(
        hash_state_ptr=hash_state_ptr, data_ptr=calldata, data_length=calldata_size
    );
    let (hash_state_ptr) = hash_update_single(hash_state_ptr=hash_state_ptr, item=max_fee);
    let (hash_state_ptr) = hash_update_single(hash_state_ptr=hash_state_ptr, item=chain_id);

    let (hash_state_ptr) = hash_update(
        hash_state_ptr=hash_state_ptr, data_ptr=additional_data, data_length=additional_data_size
    );

    let (tx_hash) = hash_finalize(hash_state_ptr=hash_state_ptr);

    return (tx_hash=tx_hash);
}

// Packs the given resource bounds in a single felt.
func pack_resource_bounds{range_check_ptr}(resource_bounds: ResourceBounds) -> felt {
    assert_nn_le(resource_bounds.max_amount, 2 ** 64 - 1);
    assert_nn(resource_bounds.max_price_per_unit);
    return (resource_bounds.resource * 2 ** 64 + resource_bounds.max_amount) * 2 ** 128 +
        resource_bounds.max_price_per_unit;
}

func hash_fee_fields{range_check_ptr, poseidon_ptr: PoseidonBuiltin*}(
    tip: felt, resource_bounds: ResourceBounds*, n_resource_bounds: felt
) -> felt {
    alloc_locals;

    let (local data_to_hash: felt*) = alloc();
    assert data_to_hash[0] = tip;
    assert_nn_le(tip, 2 ** 64 - 1);

    static_assert L1_GAS_INDEX == 0;
    static_assert L2_GAS_INDEX == 1;
    static_assert L1_DATA_GAS_INDEX == 2;

    with_attr error_message("Invalid number of resource bounds: {n_resource_bounds}.") {
        assert n_resource_bounds = 3;
    }

    // L1 gas.
    let l1_gas_bounds = resource_bounds[L1_GAS_INDEX];
    assert l1_gas_bounds.resource = L1_GAS;
    assert data_to_hash[1] = pack_resource_bounds(l1_gas_bounds);

    // L2 gas.
    let l2_gas_bounds = resource_bounds[L2_GAS_INDEX];
    assert l2_gas_bounds.resource = L2_GAS;
    assert data_to_hash[2] = pack_resource_bounds(l2_gas_bounds);

    // L1 data gas.
    let l1_data_gas_bounds = resource_bounds[L1_DATA_GAS_INDEX];
    assert l1_data_gas_bounds.resource = L1_DATA_GAS;
    assert data_to_hash[3] = pack_resource_bounds(l1_data_gas_bounds);

    let (hash) = poseidon_hash_many(n=n_resource_bounds + 1, elements=data_to_hash);
    return hash;
}

func hash_tx_common_fields{
    range_check_ptr, poseidon_ptr: PoseidonBuiltin*, hash_state: PoseidonHashState
}(common_fields: CommonTxFields*) {
    alloc_locals;

    assert common_fields.paymaster_data_length = 0;

    let fee_fields_hash = hash_fee_fields(
        tip=common_fields.tip,
        resource_bounds=common_fields.resource_bounds,
        n_resource_bounds=common_fields.n_resource_bounds,
    );

    assert common_fields.nonce_data_availability_mode = 0;
    assert common_fields.fee_data_availability_mode = 0;
    let data_availability_modes = common_fields.nonce_data_availability_mode * 2 ** 32 +
        common_fields.fee_data_availability_mode;

    poseidon_hash_update_single(item=common_fields.tx_hash_prefix);
    poseidon_hash_update_single(item=common_fields.version);
    poseidon_hash_update_single(item=common_fields.sender_address);
    poseidon_hash_update_single(item=fee_fields_hash);
    poseidon_hash_update_with_nested_hash(
        data_ptr=common_fields.paymaster_data, data_length=common_fields.paymaster_data_length
    );
    poseidon_hash_update_single(item=common_fields.chain_id);
    poseidon_hash_update_single(item=common_fields.nonce);
    poseidon_hash_update_single(item=data_availability_modes);

    return ();
}

// Note that 'execution_context.execution_info.tx_info' and 'deprecated_tx_info' are uninitialized
// when this functions is called. In particular, these fields are not used in this function.
func compute_invoke_transaction_hash{range_check_ptr, poseidon_ptr: PoseidonBuiltin*}(
    common_fields: CommonTxFields*,
    execution_context: ExecutionContext*,
    account_deployment_data_size: felt,
    account_deployment_data: felt*,
) -> felt {
    alloc_locals;

    assert account_deployment_data_size = 0;
    with_attr error_message("Invalid transaction version: {version}.") {
        assert common_fields.version = 3;
    }

    let hash_state: PoseidonHashState = poseidon_hash_init();
    with hash_state {
        hash_tx_common_fields(common_fields=common_fields);
        poseidon_hash_update_with_nested_hash(
            data_ptr=account_deployment_data, data_length=account_deployment_data_size
        );
        poseidon_hash_update_with_nested_hash(
            data_ptr=execution_context.calldata, data_length=execution_context.calldata_size
        );
    }

    let transaction_hash = poseidon_hash_finalize(hash_state=hash_state);
    return transaction_hash;
}

// See comment above `compute_invoke_transaction_hash()`.
func compute_l1_handler_transaction_hash{pedersen_ptr: HashBuiltin*}(
    execution_context: ExecutionContext*, chain_id: felt, nonce: felt
) -> felt {
    let (__fp__, _) = get_fp_and_pc();
    let (transaction_hash) = deprecated_get_transaction_hash{hash_ptr=pedersen_ptr}(
        tx_hash_prefix=L1_HANDLER_HASH_PREFIX,
        version=L1_HANDLER_VERSION,
        contract_address=execution_context.execution_info.contract_address,
        entry_point_selector=execution_context.execution_info.selector,
        calldata_size=execution_context.calldata_size,
        calldata=execution_context.calldata,
        max_fee=0,
        chain_id=chain_id,
        additional_data_size=1,
        additional_data=&nonce,
    );

    return transaction_hash;
}

// See comment above `compute_invoke_transaction_hash()`.
func compute_deploy_account_transaction_hash{range_check_ptr, poseidon_ptr: PoseidonBuiltin*}(
    common_fields: CommonTxFields*, calldata_size: felt, calldata: felt*
) -> felt {
    alloc_locals;

    with_attr error_message("Invalid transaction version: {version}.") {
        assert common_fields.version = 3;
    }

    let hash_state: PoseidonHashState = poseidon_hash_init();
    with hash_state {
        hash_tx_common_fields(common_fields=common_fields);
        // Hash and add the constructor calldata to the hash state.
        poseidon_hash_update_with_nested_hash(data_ptr=&calldata[2], data_length=calldata_size - 2);
        // Add the class hash and the contract address salt to the hash state.
        poseidon_hash_update(data_ptr=calldata, data_length=2);
    }

    let transaction_hash = poseidon_hash_finalize(hash_state=hash_state);
    return transaction_hash;
}

// See comment above `compute_invoke_transaction_hash()`.
func compute_declare_transaction_hash{range_check_ptr, poseidon_ptr: PoseidonBuiltin*}(
    common_fields: CommonTxFields*,
    class_hash: felt,
    compiled_class_hash: felt,
    account_deployment_data_size: felt,
    account_deployment_data: felt*,
) -> felt {
    alloc_locals;

    assert account_deployment_data_size = 0;
    with_attr error_message("Invalid transaction version: {version}.") {
        assert common_fields.version = 3;
    }

    let hash_state: PoseidonHashState = poseidon_hash_init();
    with hash_state {
        hash_tx_common_fields(common_fields=common_fields);
        poseidon_hash_update_with_nested_hash(
            data_ptr=account_deployment_data, data_length=account_deployment_data_size
        );
        // Add the class hash to the hash state.
        poseidon_hash_update_single(item=class_hash);
        poseidon_hash_update_single(item=compiled_class_hash);
    }
    let transaction_hash = poseidon_hash_finalize(hash_state=hash_state);

    return transaction_hash;
}

// Computes the hash of a v0 meta transaction. See the `meta_tx_v0` syscall.
func compute_meta_tx_v0_hash{pedersen_ptr: HashBuiltin*}(
    contract_address: felt,
    entry_point_selector: felt,
    calldata: felt*,
    calldata_size: felt,
    chain_id: felt,
) -> felt {
    let (tx_hash) = deprecated_get_transaction_hash{hash_ptr=pedersen_ptr}(
        tx_hash_prefix=INVOKE_HASH_PREFIX,
        version=0,
        contract_address=contract_address,
        entry_point_selector=entry_point_selector,
        calldata_size=calldata_size,
        calldata=calldata,
        max_fee=0,
        chain_id=chain_id,
        additional_data_size=0,
        additional_data=cast(0, felt*),
    );
    return tx_hash;
}

func update_pedersen_in_builtin_ptrs{builtin_ptrs: BuiltinPointers*}(pedersen_ptr: HashBuiltin*) {
    tempvar builtin_ptrs = new BuiltinPointers(
        selectable=SelectableBuiltins(
            pedersen=pedersen_ptr,
            range_check=builtin_ptrs.selectable.range_check,
            ecdsa=builtin_ptrs.selectable.ecdsa,
            bitwise=builtin_ptrs.selectable.bitwise,
            ec_op=builtin_ptrs.selectable.ec_op,
            poseidon=builtin_ptrs.selectable.poseidon,
            segment_arena=builtin_ptrs.selectable.segment_arena,
            range_check96=builtin_ptrs.selectable.range_check96,
            add_mod=builtin_ptrs.selectable.add_mod,
            mul_mod=builtin_ptrs.selectable.mul_mod,
        ),
        non_selectable=builtin_ptrs.non_selectable,
    );

    return ();
}

func update_poseidon_in_builtin_ptrs{builtin_ptrs: BuiltinPointers*}(
    poseidon_ptr: PoseidonBuiltin*
) {
    tempvar builtin_ptrs = new BuiltinPointers(
        selectable=SelectableBuiltins(
            pedersen=builtin_ptrs.selectable.pedersen,
            range_check=builtin_ptrs.selectable.range_check,
            ecdsa=builtin_ptrs.selectable.ecdsa,
            bitwise=builtin_ptrs.selectable.bitwise,
            ec_op=builtin_ptrs.selectable.ec_op,
            poseidon=poseidon_ptr,
            segment_arena=builtin_ptrs.selectable.segment_arena,
            range_check96=builtin_ptrs.selectable.range_check96,
            add_mod=builtin_ptrs.selectable.add_mod,
            mul_mod=builtin_ptrs.selectable.mul_mod,
        ),
        non_selectable=builtin_ptrs.non_selectable,
    );

    return ();
}
