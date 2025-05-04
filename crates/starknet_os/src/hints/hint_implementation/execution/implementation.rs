use std::collections::HashMap;

use blockifier::execution::contract_class::TrackedResource;
use blockifier::state::state_api::{State, StateReader};
use cairo_vm::any_box;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_ptr_from_var_name,
    insert_value_from_var_name,
    insert_value_into_ap,
};
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, ContractAddress, PatriciaKey};
use starknet_api::state::StorageKey;
use starknet_api::transaction::fields::ValidResourceBounds;
use starknet_types_core::felt::Felt;

use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::hint_implementation::execution::utils::{
    get_account_deployment_data,
    get_calldata,
};
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Const, Ids, Scope};
use crate::syscall_handler_utils::SyscallHandlerType;
use crate::vm_utils::{get_address_of_nested_fields, LoadCairoObject};

pub(crate) fn load_next_tx<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let execution_helper =
        hint_processor.execution_helpers_manager.get_mut_current_execution_helper()?;
    let tx = execution_helper.tx_tracker.load_next_tx()?;
    insert_value_from_var_name(
        Ids::TxType.into(),
        tx.tx_type().tx_type_as_felt(),
        vm,
        ids_data,
        ap_tracking,
    )?;

    // Log enter tx.
    let range_check_ptr =
        get_ptr_from_var_name(Ids::RangeCheckPtr.into(), vm, ids_data, ap_tracking)?;
    execution_helper.os_logger.enter_tx(
        tx.tx_type(),
        tx.tx_hash(),
        // TODO(Dori): when `vm.current_step` has a public getter, use it instead of the dummy
        //   value ([PR](https://github.com/lambdaclass/cairo-vm/pull/2031)).
        7,
        range_check_ptr,
        ids_data,
        vm,
        ap_tracking,
        hint_processor.os_program,
    )?;

    Ok(())
}

pub(crate) fn load_resource_bounds<S: StateReader>(
    HintArgs { vm, ids_data, ap_tracking, hint_processor, constants, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    // Guess the resource bounds.
    let resource_bounds = hint_processor
        .get_current_execution_helper()?
        .tx_tracker
        .get_account_tx()?
        .resource_bounds();
    if let ValidResourceBounds::L1Gas(_) = resource_bounds {
        return Err(OsHintError::AssertionFailed {
            message: "Only transactions with 3 resource bounds are supported. Got 1 resource \
                      bounds."
                .to_string(),
        });
    }

    let resource_bound_address = vm.add_memory_segment();
    resource_bounds.load_into(vm, hint_processor.os_program, resource_bound_address, constants)?;

    insert_value_from_var_name(
        Ids::ResourceBounds.into(),
        MaybeRelocatable::RelocatableValue(resource_bound_address),
        vm,
        ids_data,
        ap_tracking,
    )?;
    Ok(())
}

pub(crate) fn exit_tx<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let range_check_ptr =
        get_ptr_from_var_name(Ids::RangeCheckPtr.into(), vm, ids_data, ap_tracking)?;
    Ok(hint_processor
        .execution_helpers_manager
        .get_mut_current_execution_helper()?
        .os_logger
        .exit_tx(
            // TODO(Dori): when `vm.current_step` has a public getter, use it instead of the dummy
            //   value ([PR](https://github.com/lambdaclass/cairo-vm/pull/2031)).
            7,
            range_check_ptr,
            ids_data,
            vm,
            ap_tracking,
            hint_processor.os_program,
        )?)
}

pub(crate) fn prepare_constructor_execution<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn assert_transaction_hash<S: StateReader>(
    HintArgs { vm, ids_data, ap_tracking, hint_processor, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let stored_transaction_hash =
        get_integer_from_var_name(Ids::TransactionHash.into(), vm, ids_data, ap_tracking)?;
    let calculated_tx_hash =
        hint_processor.get_current_execution_helper()?.tx_tracker.get_tx()?.tx_hash().0;

    if calculated_tx_hash == stored_transaction_hash {
        Ok(())
    } else {
        Err(OsHintError::AssertionFailed {
            message: format!(
                "Computed transaction_hash is inconsistent with the hash in the transaction. \
                 Computed hash = {stored_transaction_hash:#x}, Expected hash = \
                 {calculated_tx_hash:#x}."
            ),
        })
    }
}

pub(crate) fn enter_scope_deprecated_syscall_handler<S: StateReader>(
    HintArgs { exec_scopes, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    exec_scopes.insert_value(
        Scope::SyscallHandlerType.into(),
        SyscallHandlerType::DeprecatedSyscallHandler,
    );
    Ok(())
}

pub(crate) fn enter_scope_syscall_handler<S: StateReader>(
    HintArgs { exec_scopes, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    exec_scopes.insert_value(Scope::SyscallHandlerType.into(), SyscallHandlerType::SyscallHandler);
    Ok(())
}

pub(crate) fn get_contract_address_state_entry<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn set_state_entry_to_account_contract_address<S: StateReader>(
    HintArgs { exec_scopes, vm, ids_data, ap_tracking, hint_processor, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let account_contract_address = vm
        .get_integer(get_address_of_nested_fields(
            ids_data,
            Ids::TxInfo,
            CairoStruct::TxInfoPtr,
            vm,
            ap_tracking,
            &["account_contract_address"],
            hint_processor.os_program,
        )?)?
        .into_owned();
    let state_changes_ptr =
        get_ptr_from_var_name(Ids::ContractStateChanges.into(), vm, ids_data, ap_tracking)?;
    let dict_manager = exec_scopes.get_dict_manager()?;
    let mut dict_manager_borrowed = dict_manager.borrow_mut();
    let state_entry = dict_manager_borrowed
        .get_tracker_mut(state_changes_ptr)?
        .get_value(&account_contract_address.into())?;
    insert_value_from_var_name(Ids::StateEntry.into(), state_entry, vm, ids_data, ap_tracking)?;
    Ok(())
}

pub(crate) fn get_block_hash_contract_address_state_entry_and_set_new_state_entry<
    S: StateReader,
>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn get_contract_address_state_entry_and_set_new_state_entry<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn check_is_deprecated<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, exec_scopes, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let class_hash = ClassHash(
        *vm.get_integer(
            get_address_of_nested_fields(
                ids_data,
                Ids::ExecutionContext,
                CairoStruct::ExecutionContext,
                vm,
                ap_tracking,
                &["class_hash"],
                hint_processor.os_program,
            )?
            .to_owned(),
        )?,
    );

    exec_scopes.insert_value(
        Scope::IsDeprecated.into(),
        Felt::from(hint_processor.deprecated_compiled_classes.contains_key(&class_hash)),
    );

    Ok(())
}

pub(crate) fn is_deprecated<S: StateReader>(
    HintArgs { vm, exec_scopes, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    Ok(insert_value_into_ap(vm, exec_scopes.get::<Felt>(Scope::IsDeprecated.into())?)?)
}

pub(crate) fn enter_syscall_scopes<S: StateReader>(
    HintArgs { exec_scopes, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    // Unlike the Python implementation, there is no need to add `syscall_handler`,
    // `deprecated_syscall_handler`, `deprecated_class_hashes` and `execution_helper` as scope
    // variables since they are accessible via the hint processor.
    let dict_manager = exec_scopes.get_dict_manager()?;

    let new_scope = HashMap::from([(Scope::DictManager.into(), any_box!(dict_manager))]);
    exec_scopes.enter_scope(new_scope);

    Ok(())
}

pub(crate) fn end_tx<S: StateReader>(
    HintArgs { hint_processor, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    hint_processor.get_mut_current_execution_helper()?.tx_execution_iter.end_tx()?;
    Ok(())
}

pub(crate) fn enter_call<S: StateReader>(
    HintArgs { hint_processor, ids_data, vm, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let execution_info_ptr = get_address_of_nested_fields(
        ids_data,
        Ids::ExecutionContext,
        CairoStruct::ExecutionContext,
        vm,
        ap_tracking,
        &["execution_info"],
        hint_processor.os_program,
    )?;
    let deprecated_tx_info_ptr = get_address_of_nested_fields(
        ids_data,
        Ids::ExecutionContext,
        CairoStruct::ExecutionContext,
        vm,
        ap_tracking,
        &["deprecated_tx_info"],
        hint_processor.os_program,
    )?;

    hint_processor
        .get_mut_current_execution_helper()?
        .tx_execution_iter
        .get_mut_tx_execution_info_ref()?
        .enter_call(execution_info_ptr, deprecated_tx_info_ptr)?;
    Ok(())
}

pub(crate) fn exit_call<S: StateReader>(
    HintArgs { hint_processor, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    hint_processor
        .get_mut_current_execution_helper()?
        .tx_execution_iter
        .get_mut_tx_execution_info_ref()?
        .exit_call_info()?;
    Ok(())
}

pub(crate) fn contract_address<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_calldata_len<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let calldata =
        get_calldata(hint_processor.execution_helpers_manager.get_current_execution_helper()?)?;
    insert_value_into_ap(vm, calldata.0.len())?;
    Ok(())
}

pub(crate) fn tx_calldata<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let calldata =
        get_calldata(hint_processor.execution_helpers_manager.get_current_execution_helper()?)?;
    let calldata_base = vm.gen_arg(calldata)?;
    insert_value_into_ap(vm, calldata_base)?;
    Ok(())
}

pub(crate) fn tx_entry_point_selector<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_version<S: StateReader>(HintArgs { .. }: HintArgs<'_, '_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn tx_tip<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let tip = hint_processor
        .execution_helpers_manager
        .get_current_execution_helper()?
        .tx_tracker
        .get_account_tx()?
        .tip();
    insert_value_into_ap(vm, Felt::from(tip))?;
    Ok(())
}

pub(crate) fn tx_paymaster_data_len<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let account_tx = hint_processor
        .execution_helpers_manager
        .get_current_execution_helper()?
        .tx_tracker
        .get_account_tx()?;
    let paymaster_data_len = account_tx.paymaster_data().0.len();
    insert_value_into_ap(vm, paymaster_data_len)?;
    Ok(())
}

pub(crate) fn tx_paymaster_data<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let account_tx = hint_processor
        .execution_helpers_manager
        .get_current_execution_helper()?
        .tx_tracker
        .get_account_tx()?;
    let paymaster_data: Vec<_> =
        account_tx.paymaster_data().0.into_iter().map(MaybeRelocatable::from).collect();
    let paymaster_data_base = vm.gen_arg(&paymaster_data)?;
    insert_value_into_ap(vm, paymaster_data_base)?;
    Ok(())
}

pub(crate) fn tx_nonce_data_availability_mode<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let account_tx = hint_processor
        .execution_helpers_manager
        .get_current_execution_helper()?
        .tx_tracker
        .get_account_tx()?;
    let da_mode_as_felt = Felt::from(account_tx.nonce_data_availability_mode());
    insert_value_into_ap(vm, da_mode_as_felt)?;
    Ok(())
}

pub(crate) fn tx_fee_data_availability_mode<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let account_tx = hint_processor
        .execution_helpers_manager
        .get_current_execution_helper()?
        .tx_tracker
        .get_account_tx()?;
    let da_mode_as_felt = Felt::from(account_tx.fee_data_availability_mode());
    insert_value_into_ap(vm, da_mode_as_felt)?;
    Ok(())
}

pub(crate) fn tx_account_deployment_data_len<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let account_deployment_data =
        get_account_deployment_data(hint_processor.get_current_execution_helper()?)?;
    insert_value_into_ap(vm, account_deployment_data.0.len())?;
    Ok(())
}

pub(crate) fn tx_account_deployment_data<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let account_deployment_data =
        get_account_deployment_data(hint_processor.get_current_execution_helper()?)?;
    let account_deployment_data_base = vm.gen_arg(&account_deployment_data)?;
    insert_value_into_ap(vm, account_deployment_data_base)?;
    Ok(())
}

pub(crate) fn gen_signature_arg<S: StateReader>(
    HintArgs { hint_processor, ids_data, ap_tracking, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let account_tx = hint_processor
        .execution_helpers_manager
        .get_current_execution_helper()?
        .tx_tracker
        .get_account_tx()?;
    let signature: Vec<_> = account_tx.signature().0.iter().map(MaybeRelocatable::from).collect();
    let signature_start = vm.gen_arg(&signature)?;
    insert_value_from_var_name(
        Ids::SignatureStart.into(),
        signature_start,
        vm,
        ids_data,
        ap_tracking,
    )?;
    insert_value_from_var_name(
        Ids::SignatureLen.into(),
        signature.len(),
        vm,
        ids_data,
        ap_tracking,
    )?;

    Ok(())
}

pub(crate) fn is_reverted<S: StateReader>(HintArgs { .. }: HintArgs<'_, '_, S>) -> OsHintResult {
    todo!()
}

pub(crate) fn check_execution<S: StateReader>(
    HintArgs { vm, hint_processor, ids_data, ap_tracking, constants, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let current_execution_helper =
        hint_processor.execution_helpers_manager.get_mut_current_execution_helper()?;
    if current_execution_helper.os_logger.debug {
        // Validate the predicted gas cost.
        let remaining_gas =
            get_integer_from_var_name(Ids::RemainingGas.into(), vm, ids_data, ap_tracking)?;
        let gas_builtin = vm.get_integer(get_address_of_nested_fields(
            ids_data,
            Ids::EntryPointReturnValues,
            CairoStruct::EntryPointReturnValuesPtr,
            vm,
            ap_tracking,
            &["gas_builtin"],
            hint_processor.os_program,
        )?)?;
        let actual_gas = remaining_gas - *gas_builtin;

        let call_info = current_execution_helper
            .tx_execution_iter
            .get_tx_execution_info_ref()?
            .get_call_info_tracker()?
            .call_info;
        let mut predicted = Felt::from(call_info.execution.gas_consumed);

        match call_info.tracked_resource {
            TrackedResource::SierraGas => {
                let initial_budget = Const::EntryPointInitialBudget.fetch(constants)?;
                predicted -= initial_budget;
                if actual_gas != predicted {
                    return Err(OsHintError::AssertionFailed {
                        message: format!(
                            "Predicted gas costs are inconsistent with the actual execution; \
                             predicted={predicted}, actual={actual_gas}.",
                        ),
                    });
                }
            }
            TrackedResource::CairoSteps => {
                if predicted != Felt::ZERO {
                    return Err(OsHintError::AssertionFailed {
                        message: "Predicted gas cost must be zero in CairoSteps mode.".to_string(),
                    });
                }
            }
        };
    }

    let syscall_ptr_end = get_address_of_nested_fields(
        ids_data,
        Ids::EntryPointReturnValues,
        CairoStruct::EntryPointReturnValuesPtr,
        vm,
        ap_tracking,
        &["syscall_ptr"],
        hint_processor.os_program,
    )?;
    hint_processor.syscall_hint_processor.validate_and_discard_syscall_ptr(&syscall_ptr_end)?;
    current_execution_helper.tx_execution_iter.get_mut_tx_execution_info_ref()?.exit_call_info()?;
    Ok(())
}

pub(crate) fn is_remaining_gas_lt_initial_budget<S: StateReader>(
    HintArgs { vm, ids_data, ap_tracking, constants, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let remaining_gas =
        get_integer_from_var_name(Ids::RemainingGas.into(), vm, ids_data, ap_tracking)?;
    let initial_budget = Const::EntryPointInitialBudget.fetch(constants)?;
    let remaining_gas_lt_initial_budget: Felt = (&remaining_gas < initial_budget).into();
    Ok(insert_value_into_ap(vm, remaining_gas_lt_initial_budget)?)
}

pub(crate) fn check_syscall_response<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn check_new_syscall_response<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn check_new_deploy_response<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn log_enter_syscall<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn initial_ge_required_gas<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn set_ap_to_tx_nonce<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let nonce = hint_processor
        .execution_helpers_manager
        .get_current_execution_helper()?
        .tx_tracker
        .get_account_tx()?
        .nonce();
    insert_value_into_ap(vm, nonce.0)?;
    Ok(())
}

pub(crate) fn set_fp_plus_4_to_tx_nonce<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

fn write_syscall_result_helper<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, exec_scopes, .. }: HintArgs<'_, '_, S>,
    ids_type: Ids,
    struct_type: CairoStruct,
    key_name: &str,
) -> OsHintResult {
    let key = StorageKey(PatriciaKey::try_from(
        vm.get_integer(get_address_of_nested_fields(
            ids_data,
            ids_type,
            struct_type,
            vm,
            ap_tracking,
            &[key_name],
            hint_processor.os_program,
        )?)?
        .into_owned(),
    )?);

    let contract_address = ContractAddress(
        get_integer_from_var_name(Ids::ContractAddress.into(), vm, ids_data, ap_tracking)?
            .try_into()?,
    );

    let current_execution_helper =
        hint_processor.execution_helpers_manager.get_mut_current_execution_helper()?;
    let prev_value = current_execution_helper.cached_state.get_storage_at(contract_address, key)?;

    insert_value_from_var_name(Ids::PrevValue.into(), prev_value, vm, ids_data, ap_tracking)?;

    let request_value = vm
        .get_integer(get_address_of_nested_fields(
            ids_data,
            ids_type,
            struct_type,
            vm,
            ap_tracking,
            &["value"],
            hint_processor.os_program,
        )?)?
        .into_owned();

    current_execution_helper.cached_state.set_storage_at(contract_address, key, request_value)?;

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

    Ok(())
}

pub(crate) fn write_syscall_result_deprecated<S: StateReader>(
    hint_args: HintArgs<'_, '_, S>,
) -> OsHintResult {
    write_syscall_result_helper(hint_args, Ids::SyscallPtr, CairoStruct::StorageWritePtr, "address")
}

pub(crate) fn write_syscall_result<S: StateReader>(hint_args: HintArgs<'_, '_, S>) -> OsHintResult {
    write_syscall_result_helper(hint_args, Ids::Request, CairoStruct::StorageReadRequestPtr, "key")
}

pub(crate) fn declare_tx_fields<S: StateReader>(
    HintArgs { .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    todo!()
}

pub(crate) fn write_old_block_to_storage<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, constants, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let execution_helper = &mut hint_processor.get_mut_current_execution_helper()?;

    let block_hash_contract_address = Const::BlockHashContractAddress.fetch(constants)?;
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
    HintArgs { hint_processor, vm, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
    id: Ids,
    cairo_struct_type: CairoStruct,
    nested_fields: &[&str],
) -> OsHintResult {
    let key = StorageKey(PatriciaKey::try_from(
        vm.get_integer(get_address_of_nested_fields(
            ids_data,
            id,
            cairo_struct_type,
            vm,
            ap_tracking,
            nested_fields,
            hint_processor.os_program,
        )?)?
        .into_owned(),
    )?);

    let contract_address = ContractAddress(
        get_integer_from_var_name(Ids::ContractAddress.into(), vm, ids_data, ap_tracking)?
            .try_into()?,
    );

    let value = hint_processor
        .get_current_execution_helper()?
        .cached_state
        .get_storage_at(contract_address, key)?;

    let ids_value = get_integer_from_var_name(Ids::Value.into(), vm, ids_data, ap_tracking)?;

    if value != ids_value {
        return Err(OsHintError::InconsistentValue { expected: value, actual: ids_value });
    }
    Ok(())
}

pub(crate) fn cache_contract_storage_request_key<S: StateReader>(
    hint_args: HintArgs<'_, '_, S>,
) -> OsHintResult {
    assert_value_cached_by_reading(
        hint_args,
        Ids::Request,
        CairoStruct::StorageReadRequestPtr,
        &["key"],
    )
}

pub(crate) fn cache_contract_storage_syscall_request_address<S: StateReader>(
    hint_args: HintArgs<'_, '_, S>,
) -> OsHintResult {
    assert_value_cached_by_reading(
        hint_args,
        Ids::SyscallPtr,
        CairoStruct::StorageReadPtr,
        &["request", "key"],
    )
}

pub(crate) fn get_old_block_number_and_hash<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, constants, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let os_input = &hint_processor.get_current_execution_helper()?.os_block_input;
    let (old_block_number, old_block_hash) =
        os_input.old_block_number_and_hash.ok_or(OsHintError::BlockNumberTooSmall {
            stored_block_hash_buffer: *Const::StoredBlockHashBuffer.fetch(constants)?,
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

pub(crate) fn fetch_result<S: StateReader>(HintArgs { .. }: HintArgs<'_, '_, S>) -> OsHintResult {
    todo!()
}
