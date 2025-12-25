from starkware.cairo.common.dict import dict_update
from starkware.cairo.common.dict_access import DictAccess
from starkware.cairo.common.math import assert_not_zero
from starkware.starknet.common.constants import ORIGIN_ADDRESS
from starkware.starknet.core.os.block_context import BlockContext
from starkware.starknet.core.os.builtins import BuiltinPointers
from starkware.starknet.core.os.constants import (
    ALIAS_CONTRACT_ADDRESS,
    BLOCK_HASH_CONTRACT_ADDRESS,
    RESERVED_CONTRACT_ADDRESS,
)
from starkware.starknet.core.os.execution.deprecated_execute_entry_point import (
    select_execute_entry_point_func,
)
from starkware.starknet.core.os.execution.execute_entry_point import ExecutionContext
from starkware.starknet.core.os.execution.revert import (
    CHANGE_CLASS_ENTRY,
    CHANGE_CONTRACT_ENTRY,
    RevertLogEntry,
)
from starkware.starknet.core.os.output import OsCarriedOutputs
from starkware.starknet.core.os.state.commitment import UNINITIALIZED_CLASS_HASH, StateEntry

// Deploys a contract and invokes its constructor.
// Returns the constructor's return data.
//
// Arguments:
// block_context - A global context that is fixed throughout the block.
// constructor_execution_context - The ExecutionContext of the constructor.
// TODO(Yoni, 1/1/2027): handle failures.
func deploy_contract{
    range_check_ptr,
    remaining_gas: felt,
    builtin_ptrs: BuiltinPointers*,
    contract_state_changes: DictAccess*,
    contract_class_changes: DictAccess*,
    revert_log: RevertLogEntry*,
    outputs: OsCarriedOutputs*,
}(block_context: BlockContext*, constructor_execution_context: ExecutionContext*) -> (
    retdata_size: felt, retdata: felt*
) {
    alloc_locals;

    local contract_address = constructor_execution_context.execution_info.contract_address;

    // Assert that we don't deploy to one of the reserved addresses.
    assert_not_zero(
        (contract_address - ORIGIN_ADDRESS) * (contract_address - BLOCK_HASH_CONTRACT_ADDRESS) * (
            contract_address - ALIAS_CONTRACT_ADDRESS
        ) * (contract_address - RESERVED_CONTRACT_ADDRESS),
    );

    local state_entry: StateEntry*;
    %{ GetContractAddressStateEntry %}
    assert state_entry.class_hash = UNINITIALIZED_CLASS_HASH;
    assert state_entry.nonce = 0;

    tempvar new_state_entry = new StateEntry(
        class_hash=constructor_execution_context.class_hash,
        storage_ptr=state_entry.storage_ptr,
        nonce=0,
    );

    dict_update{dict_ptr=contract_state_changes}(
        key=contract_address,
        prev_value=cast(state_entry, felt),
        new_value=cast(new_state_entry, felt),
    );

    // Entries before this point belong to the caller.
    // Note the caller is the deployer (not the caller of the deployer).
    assert [revert_log] = RevertLogEntry(
        selector=CHANGE_CONTRACT_ENTRY,
        value=constructor_execution_context.execution_info.caller_address,
    );
    let revert_log = &revert_log[1];

    assert [revert_log] = RevertLogEntry(
        selector=CHANGE_CLASS_ENTRY, value=UNINITIALIZED_CLASS_HASH
    );
    let revert_log = &revert_log[1];

    // Invoke the contract constructor.
    let (is_reverted, retdata_size, retdata, _is_deprecated) = select_execute_entry_point_func(
        block_context=block_context, execution_context=constructor_execution_context
    );

    // Entries before this point belong to the deployed contract.
    assert [revert_log] = RevertLogEntry(selector=CHANGE_CONTRACT_ENTRY, value=contract_address);
    let revert_log = &revert_log[1];

    // The deprecated deploy syscalls do not support reverts.
    assert is_reverted = 0;
    return (retdata_size=retdata_size, retdata=retdata);
}
