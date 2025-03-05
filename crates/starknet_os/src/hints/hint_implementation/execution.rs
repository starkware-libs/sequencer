use blockifier::state::state_api::{State, StateReader};
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_constant_from_var_name,
    get_integer_from_var_name,
    get_ptr_from_var_name,
    insert_value_from_var_name,
};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ContractAddress, PatriciaKey};
use starknet_api::state::StorageKey;

use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Const, Ids};
use crate::vm_utils::get_address_of_nested_fields;

pub(crate) fn load_next_tx<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn exit_tx<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn prepare_constructor_execution<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn transaction_version<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn assert_transaction_hash<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn enter_scope_deprecated_syscall_handler<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn enter_scope_syscall_handler<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn get_contract_address_state_entry<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn set_state_entry_to_account_contract_address<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn get_block_hash_contract_address_state_entry_and_set_new_state_entry<
    S: StateReader,
>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn get_contract_address_state_entry_and_set_new_state_entry<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn check_is_deprecated<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn is_deprecated<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn enter_syscall_scopes<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn end_tx<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    // TODO(lior): No longer equivalent to moonsong impl; PTAL the new implementation of
    //   end_tx().
    todo!()
}

pub(crate) fn enter_call<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    // TODO(lior): No longer equivalent to moonsong impl; PTAL the new implementation of
    //   enter_call().
    todo!()
}

pub(crate) fn exit_call<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    // TODO(lior): No longer equivalent to moonsong impl; PTAL the new implementation of
    //   exit_call().
    todo!()
}

pub(crate) fn contract_address<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_calldata_len<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_calldata<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_entry_point_selector<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_max_fee<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_nonce<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_tip<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_paymaster_data_len<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_paymaster_data<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_nonce_data_availability_mode<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_fee_data_availability_mode<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_account_deployment_data_len<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_account_deployment_data<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn gen_signature_arg<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn is_reverted<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn check_execution<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn is_remaining_gas_lt_initial_budget<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn check_syscall_response<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn check_new_syscall_response<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn check_new_deploy_response<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn log_enter_syscall<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn initial_ge_required_gas<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn set_ap_to_tx_nonce<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn set_fp_plus_4_to_tx_nonce<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn enter_scope_node<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn enter_scope_new_node<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn enter_scope_next_node_bit_0<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn enter_scope_next_node_bit_1<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn enter_scope_left_child<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn enter_scope_right_child<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn enter_scope_descend_edge<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn write_syscall_result_deprecated<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, S>,
) -> OsHintResult {
    todo!()
}

// pub const WRITE_SYSCALL_RESULT: &str = indoc! {r#"
//     storage = execution_helper.storage_by_address[ids.contract_address]
//     ids.prev_value = storage.read(key=ids.request.key)
//     storage.write(key=ids.request.key, value=ids.request.value)

//     # Fetch a state_entry in this hint and validate it in the update that comes next.
//     ids.state_entry = __dict_manager.get_dict(ids.contract_state_changes)[ids.contract_address]
//     ids.new_state_entry = segments.add()"#
// };

// pub async fn write_syscall_result_async<PCS>(
//     vm: &mut VirtualMachine,
//     exec_scopes: &mut ExecutionScopes,
//     ids_data: &HashMap<String, HintReference>,
//     ap_tracking: &ApTracking,
// ) -> Result<(), HintError>
// where
//     PCS: PerContractStorage + 'static,
// {
//     let mut execution_helper: ExecutionHelperWrapper<PCS> =
// exec_scopes.get(vars::scopes::EXECUTION_HELPER)?;

//     let contract_address = get_integer_from_var_name(vars::ids::CONTRACT_ADDRESS, vm, ids_data,
// ap_tracking)?;     let request = get_ptr_from_var_name(vars::ids::REQUEST, vm, ids_data,
// ap_tracking)?;     let storage_write_address = *vm.get_integer((request +
// new_syscalls::StorageWriteRequest::key_offset())?)?;     let storage_write_value =
//         vm.get_integer((request +
// new_syscalls::StorageWriteRequest::value_offset())?)?.into_owned();

//     // ids.prev_value = storage.read(key=ids.request.key)
//     let prev_value =
//         execution_helper.read_storage_for_address(contract_address,
// storage_write_address).await.unwrap_or_default();
//     insert_value_from_var_name(vars::ids::PREV_VALUE, prev_value, vm, ids_data, ap_tracking)?;

//     // storage.write(key=ids.request.key, value=ids.request.value)
//     execution_helper
//         .write_storage_for_address(contract_address, storage_write_address, storage_write_value)
//         .await
//         .map_err(|e| custom_hint_error(format!("Failed to write storage for contract {}: {e}",
// contract_address)))?;

//     let contract_state_changes = get_ptr_from_var_name(vars::ids::CONTRACT_STATE_CHANGES, vm,
// ids_data, ap_tracking)?;     get_state_entry_and_set_new_state_entry(
//         contract_state_changes,
//         contract_address,
//         vm,
//         exec_scopes,
//         ids_data,
//         ap_tracking,
//     )?;

//     Ok(())
// }

// pub fn write_syscall_result<PCS>(
//     vm: &mut VirtualMachine,
//     exec_scopes: &mut ExecutionScopes,
//     ids_data: &HashMap<String, HintReference>,
//     ap_tracking: &ApTracking,
//     _constants: &HashMap<String, Felt252>,
// ) -> Result<(), HintError>
// where
//     PCS: PerContractStorage + 'static,
// {
//     execute_coroutine(write_syscall_result_async::<PCS>(vm, exec_scopes, ids_data, ap_tracking))?
// }

pub(crate) fn write_syscall_result<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, exec_scopes, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let key = StorageKey(PatriciaKey::try_from(
        vm.get_integer(get_address_of_nested_fields(
            ids_data,
            Ids::Request,
            CairoStruct::StorageReadRequestPtr,
            vm,
            ap_tracking,
            &["key".to_string()],
            &hint_processor.execution_helper.os_program,
        )?)?
        .into_owned(),
    )?);

    let contract_address = ContractAddress(
        get_integer_from_var_name(Ids::ContractAddress.into(), vm, ids_data, ap_tracking)?
            .try_into()?,
    );

    let prev_value =
        hint_processor.execution_helper.cached_state.get_storage_at(contract_address, key)?;

    insert_value_from_var_name(Ids::Value.into(), prev_value, vm, ids_data, ap_tracking)?;

    let ids_request_value = vm
        .get_integer(get_address_of_nested_fields(
            ids_data,
            Ids::Request,
            CairoStruct::StorageReadRequestPtr,
            vm,
            ap_tracking,
            &["value".to_string()],
            &hint_processor.execution_helper.os_program,
        )?)?
        .into_owned();

    hint_processor.execution_helper.cached_state.set_storage_at(
        contract_address,
        key,
        ids_request_value,
    )?;

    // Fetch a state_entry in this hint and validate it in the update that comes next.

    let contract_state_changes_ptr =
        get_ptr_from_var_name(Ids::ContractStateChanges.into(), vm, ids_data, ap_tracking)?;
    let dict_manager = exec_scopes.get_dict_manager()?;
    let mut dict_manager_borrowed = dict_manager.borrow_mut();
    let contract_address_state_entry = dict_manager_borrowed
        .get_tracker_mut(contract_state_changes_ptr)?
        .get_value(&contract_address.key().into())?;

    insert_value_from_var_name(
        Ids::StateEntry.into(),
        contract_address_state_entry,
        vm,
        ids_data,
        ap_tracking,
    )?;

    insert_value_from_var_name(
        Ids::NewStateEntry.into(),
        vm.add_memory_segment(),
        vm,
        ids_data,
        ap_tracking,
    )?;

    Ok(())
}

pub(crate) fn gen_class_hash_arg<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn write_old_block_to_storage<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, constants, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let execution_helper = &mut hint_processor.execution_helper;

    let block_hash_contract_address =
        get_constant_from_var_name(Const::BlockHashContractAddress.into(), constants)?;
    let old_block_number =
        get_integer_from_var_name(Ids::OldBlockNumber.into(), vm, ids_data, ap_tracking)?;
    let old_block_hash =
        get_integer_from_var_name(Ids::OldBlockHash.into(), vm, ids_data, ap_tracking)?;

    log::debug!("writing block number: {} -> block hash: {}", old_block_number, old_block_hash);

    execution_helper.cached_state.set_storage_at(
        ContractAddress(PatriciaKey::try_from(*block_hash_contract_address)?),
        StorageKey(PatriciaKey::try_from(old_block_number)?),
        old_block_hash,
    )?;
    Ok(())
}

fn assert_value_cached_by_reading<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, .. }: HintArgs<'_, S>,
    cairo_struct_type: CairoStruct,
    nested_fields: &[String],
) -> OsHintResult {
    let key = StorageKey(PatriciaKey::try_from(
        vm.get_integer(get_address_of_nested_fields(
            ids_data,
            Ids::Request,
            cairo_struct_type,
            vm,
            ap_tracking,
            nested_fields,
            &hint_processor.execution_helper.os_program,
        )?)?
        .into_owned(),
    )?);

    let contract_address = ContractAddress(
        get_integer_from_var_name(Ids::ContractAddress.into(), vm, ids_data, ap_tracking)?
            .try_into()?,
    );

    let value =
        hint_processor.execution_helper.cached_state.get_storage_at(contract_address, key)?;

    let ids_value = get_integer_from_var_name(Ids::Value.into(), vm, ids_data, ap_tracking)?;

    if value != ids_value {
        return Err(OsHintError::InconsistentValue { expected: value, actual: ids_value });
    }
    Ok(())
}

pub(crate) fn cache_contract_storage_request_key<S: StateReader>(
    hint_args: HintArgs<'_, S>,
) -> OsHintResult {
    assert_value_cached_by_reading(
        hint_args,
        CairoStruct::StorageReadRequestPtr,
        &["key".to_string()],
    )
}

pub(crate) fn cache_contract_storage_syscall_request_address<S: StateReader>(
    hint_args: HintArgs<'_, S>,
) -> OsHintResult {
    assert_value_cached_by_reading(
        hint_args,
        CairoStruct::StorageReadPtr,
        &["request".to_string(), "key".to_string()],
    )
}

pub(crate) fn get_old_block_number_and_hash<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, constants, .. }: HintArgs<'_, S>,
) -> OsHintResult {
    let os_input = &hint_processor.execution_helper.os_input;
    let (old_block_number, old_block_hash) =
        os_input.old_block_number_and_hash.ok_or(OsHintError::BlockNumberTooSmall {
            stored_block_hash_buffer: *get_constant_from_var_name(
                Const::StoredBlockHashBuffer.into(),
                constants,
            )?,
        })?;

    let ids_old_block_number = BlockNumber(
        get_integer_from_var_name(Ids::OldBlockNumber.into(), vm, ids_data, ap_tracking)?
            .try_into()
            .expect("Block number should fit in u64"),
    );
    if old_block_number != ids_old_block_number {
        return Err(OsHintError::InconsistentBlockNumber {
            expected: old_block_number,
            actual: ids_old_block_number,
        });
    }
    insert_value_from_var_name(
        Ids::OldBlockHash.into(),
        old_block_hash.0,
        vm,
        ids_data,
        ap_tracking,
    )?;
    Ok(())
}

pub(crate) fn fetch_result<S: StateReader>(HintArgs { .. }: HintArgs<'_, S>) -> OsHintResult {
    todo!()
}
