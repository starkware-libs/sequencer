use std::borrow::Cow;
use std::cmp::min;
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
use cairo_vm::hint_processor::hint_processor_utils::felt_to_usize;
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, ContractAddress, PatriciaKey};
use starknet_api::executable_transaction::{AccountTransaction, Transaction};
use starknet_api::state::StorageKey;
use starknet_api::transaction::fields::ValidResourceBounds;
use starknet_api::transaction::{DeployAccountTransaction, TransactionVersion};
use starknet_types_core::felt::Felt;

use crate::hints::enum_definition::{AllHints, OsHint};
use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::hint_implementation::execution::utils::{
    assert_retdata_as_expected,
    compare_retdata,
    extract_actual_retdata,
    get_account_deployment_data,
    get_calldata,
    set_state_entry,
};
use crate::hints::nondet_offsets::insert_nondet_hint_value;
use crate::hints::types::HintArgs;
use crate::hints::vars::{CairoStruct, Const, Ids, Scope};
use crate::syscall_handler_utils::SyscallHandlerType;
use crate::vm_utils::{
    get_address_of_nested_fields,
    get_address_of_nested_fields_from_base_address,
    LoadCairoObject,
};

#[allow(clippy::result_large_err)]
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
        vm.get_current_step(),
        range_check_ptr,
        ids_data,
        vm,
        ap_tracking,
        hint_processor.os_program,
    )?;

    Ok(())
}

#[allow(clippy::result_large_err)]
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

#[allow(clippy::result_large_err)]
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
            vm.get_current_step(),
            range_check_ptr,
            ids_data,
            vm,
            ap_tracking,
            hint_processor.os_program,
        )?)
}

#[allow(clippy::result_large_err)]
pub(crate) fn prepare_constructor_execution<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let account_tx = hint_processor
        .execution_helpers_manager
        .get_mut_current_execution_helper()?
        .tx_tracker
        .get_account_tx()?;
    let AccountTransaction::DeployAccount(deploy_account_tx) = account_tx else {
        return Err(OsHintError::UnexpectedTxType(account_tx.tx_type()));
    };

    insert_value_from_var_name(
        Ids::ContractAddressSalt.into(),
        deploy_account_tx.contract_address_salt().0,
        vm,
        ids_data,
        ap_tracking,
    )?;
    insert_value_from_var_name(
        Ids::ClassHash.into(),
        deploy_account_tx.class_hash().0,
        vm,
        ids_data,
        ap_tracking,
    )?;

    let constructor_calldata = match &deploy_account_tx.tx {
        DeployAccountTransaction::V1(v1_tx) => &v1_tx.constructor_calldata,
        DeployAccountTransaction::V3(v3_tx) => &v3_tx.constructor_calldata,
    };
    insert_value_from_var_name(
        Ids::ConstructorCalldataSize.into(),
        constructor_calldata.0.len(),
        vm,
        ids_data,
        ap_tracking,
    )?;
    let constructor_calldata_base = vm.add_memory_segment();
    let constructor_calldata_as_relocatable: Vec<MaybeRelocatable> =
        constructor_calldata.0.iter().map(MaybeRelocatable::from).collect();
    vm.load_data(constructor_calldata_base, &constructor_calldata_as_relocatable)?;
    insert_value_from_var_name(
        Ids::ConstructorCalldata.into(),
        constructor_calldata_base,
        vm,
        ids_data,
        ap_tracking,
    )?;
    Ok(())
}

#[allow(clippy::result_large_err)]
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

#[allow(clippy::result_large_err)]
pub(crate) fn enter_scope_deprecated_syscall_handler<S: StateReader>(
    HintArgs { exec_scopes, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let new_scope = HashMap::from([(
        Scope::SyscallHandlerType.into(),
        any_box!(SyscallHandlerType::DeprecatedSyscallHandler),
    )]);
    exec_scopes.enter_scope(new_scope);
    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn enter_scope_syscall_handler<S: StateReader>(
    HintArgs { exec_scopes, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let new_scope = HashMap::from([(
        Scope::SyscallHandlerType.into(),
        any_box!(SyscallHandlerType::SyscallHandler),
    )]);
    exec_scopes.enter_scope(new_scope);
    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn get_contract_address_state_entry<S: StateReader>(
    HintArgs { exec_scopes, vm, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let contract_address =
        get_integer_from_var_name(Ids::ContractAddress.into(), vm, ids_data, ap_tracking)?;
    set_state_entry(&contract_address, vm, exec_scopes, ids_data, ap_tracking)?;
    Ok(())
}

#[allow(clippy::result_large_err)]
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
    set_state_entry(&account_contract_address, vm, exec_scopes, ids_data, ap_tracking)?;
    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn get_block_hash_contract_address_state_entry_and_set_new_state_entry<
    S: StateReader,
>(
    HintArgs { vm, exec_scopes, constants, ap_tracking, ids_data, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let block_hash_contract_address = Const::BlockHashContractAddress.fetch(constants)?;
    set_state_entry(block_hash_contract_address, vm, exec_scopes, ids_data, ap_tracking)
}

#[allow(clippy::result_large_err)]
pub(crate) fn check_is_deprecated<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, exec_scopes, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let class_hash = ClassHash(
        *vm.get_integer(
            get_address_of_nested_fields(
                ids_data,
                Ids::ExecutionContext,
                CairoStruct::ExecutionContextPtr,
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

#[allow(clippy::result_large_err)]
pub(crate) fn is_deprecated<S: StateReader>(
    HintArgs { vm, exec_scopes, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    Ok(insert_value_into_ap(vm, exec_scopes.get::<Felt>(Scope::IsDeprecated.into())?)?)
}

#[allow(clippy::result_large_err)]
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

#[allow(clippy::result_large_err)]
pub(crate) fn end_tx<S: StateReader>(
    HintArgs { hint_processor, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    hint_processor.get_mut_current_execution_helper()?.tx_execution_iter.end_tx()?;
    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn enter_call<S: StateReader>(
    HintArgs { hint_processor, ids_data, vm, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let execution_info_ptr = vm.get_relocatable(get_address_of_nested_fields(
        ids_data,
        Ids::ExecutionContext,
        CairoStruct::ExecutionContextPtr,
        vm,
        ap_tracking,
        &["execution_info"],
        hint_processor.os_program,
    )?)?;
    let deprecated_tx_info_ptr = vm.get_relocatable(get_address_of_nested_fields(
        ids_data,
        Ids::ExecutionContext,
        CairoStruct::ExecutionContextPtr,
        vm,
        ap_tracking,
        &["deprecated_tx_info"],
        hint_processor.os_program,
    )?)?;

    hint_processor
        .get_mut_current_execution_helper()?
        .tx_execution_iter
        .get_mut_tx_execution_info_ref()?
        .enter_call(execution_info_ptr, deprecated_tx_info_ptr)?;
    Ok(())
}

#[allow(clippy::result_large_err)]
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

#[allow(clippy::result_large_err)]
pub(crate) fn contract_address<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let tx = hint_processor.get_current_execution_helper()?.tx_tracker.get_tx()?;
    let contract_address = match tx {
        Transaction::Account(account_tx) => account_tx.sender_address(),
        Transaction::L1Handler(l1_handler) => l1_handler.tx.contract_address,
    };
    insert_value_from_var_name(
        Ids::ContractAddress.into(),
        **contract_address,
        vm,
        ids_data,
        ap_tracking,
    )?;
    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn tx_calldata_len<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let calldata =
        get_calldata(hint_processor.execution_helpers_manager.get_current_execution_helper()?)?;
    insert_value_into_ap(vm, calldata.0.len())?;
    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn tx_calldata<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let calldata: Vec<_> =
        get_calldata(hint_processor.execution_helpers_manager.get_current_execution_helper()?)?
            .0
            .iter()
            .map(MaybeRelocatable::from)
            .collect();
    let calldata_base = vm.gen_arg(&calldata)?;
    insert_value_into_ap(vm, calldata_base)?;
    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn tx_entry_point_selector<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let tx = hint_processor
        .execution_helpers_manager
        .get_current_execution_helper()?
        .tx_tracker
        .get_tx()?;
    let entry_point_selector = match tx {
        Transaction::L1Handler(l1_handler) => l1_handler.tx.entry_point_selector,
        _ => {
            return Err(OsHintError::UnexpectedTxType(tx.tx_type()));
        }
    };
    insert_value_into_ap(vm, entry_point_selector.0)?;
    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn tx_version<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let version = hint_processor.get_current_execution_helper()?.tx_tracker.get_tx()?.version();
    insert_value_into_ap(vm, version.0)?;
    Ok(())
}

#[allow(clippy::result_large_err)]
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

#[allow(clippy::result_large_err)]
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

#[allow(clippy::result_large_err)]
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

#[allow(clippy::result_large_err)]
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

#[allow(clippy::result_large_err)]
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

#[allow(clippy::result_large_err)]
pub(crate) fn tx_account_deployment_data_len<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let account_deployment_data =
        get_account_deployment_data(hint_processor.get_current_execution_helper()?)?;
    insert_nondet_hint_value(
        vm,
        AllHints::OsHint(OsHint::TxAccountDeploymentDataLen),
        account_deployment_data.0.len(),
    )?;
    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn tx_account_deployment_data<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let account_deployment_data: Vec<_> =
        get_account_deployment_data(hint_processor.get_current_execution_helper()?)?
            .0
            .iter()
            .map(MaybeRelocatable::from)
            .collect();
    let account_deployment_data_base = vm.gen_arg(&account_deployment_data)?;
    insert_value_into_ap(vm, account_deployment_data_base)?;
    Ok(())
}

#[allow(clippy::result_large_err)]
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

#[allow(clippy::result_large_err)]
pub(crate) fn is_reverted<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let is_reverted = hint_processor
        .execution_helpers_manager
        .get_current_execution_helper()?
        .tx_execution_iter
        .get_tx_execution_info_ref()?
        .tx_execution_info
        .is_reverted();
    insert_value_into_ap(vm, Felt::from(is_reverted))?;
    Ok(())
}

#[allow(clippy::result_large_err)]
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

    let syscall_ptr_end_address = get_address_of_nested_fields(
        ids_data,
        Ids::EntryPointReturnValues,
        CairoStruct::EntryPointReturnValuesPtr,
        vm,
        ap_tracking,
        &["syscall_ptr"],
        hint_processor.os_program,
    )?;
    let syscall_ptr_end = vm.get_relocatable(syscall_ptr_end_address)?;
    hint_processor.syscall_hint_processor.validate_and_discard_syscall_ptr(&syscall_ptr_end)?;
    current_execution_helper.tx_execution_iter.get_mut_tx_execution_info_ref()?.exit_call_info()?;
    Ok(())
}

#[allow(clippy::result_large_err)]
pub(crate) fn is_remaining_gas_lt_initial_budget<S: StateReader>(
    HintArgs { vm, ids_data, ap_tracking, constants, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let remaining_gas =
        get_integer_from_var_name(Ids::RemainingGas.into(), vm, ids_data, ap_tracking)?;
    let initial_budget = Const::EntryPointInitialBudget.fetch(constants)?;
    let remaining_gas_lt_initial_budget: Felt = (&remaining_gas < initial_budget).into();
    Ok(insert_value_into_ap(vm, remaining_gas_lt_initial_budget)?)
}

#[allow(clippy::result_large_err)]
pub(crate) fn check_syscall_response<S: StateReader>(
    HintArgs { hint_processor, vm, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let actual_retdata = extract_actual_retdata(vm, ids_data, ap_tracking)?;
    let call_response_ptr =
        get_ptr_from_var_name(Ids::CallResponse.into(), vm, ids_data, ap_tracking)?;
    let retdata_size = vm.get_integer(get_address_of_nested_fields_from_base_address(
        call_response_ptr,
        CairoStruct::CallContractResponse,
        vm,
        &["retdata_size"],
        hint_processor.os_program,
    )?)?;
    let retdata_base = vm.get_relocatable(get_address_of_nested_fields_from_base_address(
        call_response_ptr,
        CairoStruct::CallContractResponse,
        vm,
        &["retdata"],
        hint_processor.os_program,
    )?)?;
    let expected_retdata = vm.get_continuous_range(retdata_base, felt_to_usize(&retdata_size)?)?;
    compare_retdata(&actual_retdata, &expected_retdata)
}

#[allow(clippy::result_large_err)]
pub(crate) fn check_new_syscall_response<S: StateReader>(
    HintArgs { hint_processor, vm, ap_tracking, ids_data, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    assert_retdata_as_expected(
        "retdata_start",
        "retdata_end",
        CairoStruct::CallContractResponse,
        vm,
        ap_tracking,
        ids_data,
        hint_processor.os_program,
    )
}

#[allow(clippy::result_large_err)]
pub(crate) fn check_new_deploy_response<S: StateReader>(
    HintArgs { hint_processor, vm, ap_tracking, ids_data, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    assert_retdata_as_expected(
        "constructor_retdata_start",
        "constructor_retdata_end",
        CairoStruct::DeployResponse,
        vm,
        ap_tracking,
        ids_data,
        hint_processor.os_program,
    )
}

#[allow(clippy::result_large_err)]
pub(crate) fn initial_ge_required_gas<S: StateReader>(
    HintArgs { vm, ids_data, ap_tracking, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let initial_gas = get_integer_from_var_name(Ids::InitialGas.into(), vm, ids_data, ap_tracking)?;
    let required_gas =
        get_integer_from_var_name(Ids::RequiredGas.into(), vm, ids_data, ap_tracking)?;
    insert_value_into_ap(vm, Felt::from(initial_gas >= required_gas))?;
    Ok(())
}

#[allow(clippy::result_large_err)]
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

#[allow(clippy::result_large_err)]
pub(crate) fn set_fp_plus_4_to_tx_nonce<S: StateReader>(
    HintArgs { hint_processor, vm, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let nonce = hint_processor
        .execution_helpers_manager
        .get_current_execution_helper()?
        .tx_tracker
        .get_tx()?
        .nonce();
    insert_nondet_hint_value(vm, AllHints::OsHint(OsHint::SetFpPlus4ToTxNonce), nonce.0)?;
    Ok(())
}

#[allow(clippy::result_large_err)]
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

    set_state_entry(contract_address.key(), vm, exec_scopes, ids_data, ap_tracking)
}

#[allow(clippy::result_large_err)]
pub(crate) fn write_syscall_result_deprecated<S: StateReader>(
    hint_args: HintArgs<'_, '_, S>,
) -> OsHintResult {
    write_syscall_result_helper(hint_args, Ids::SyscallPtr, CairoStruct::StorageWritePtr, "address")
}

#[allow(clippy::result_large_err)]
pub(crate) fn write_syscall_result<S: StateReader>(hint_args: HintArgs<'_, '_, S>) -> OsHintResult {
    write_syscall_result_helper(hint_args, Ids::Request, CairoStruct::StorageWriteRequest, "key")
}

#[allow(clippy::result_large_err)]
pub(crate) fn declare_tx_fields<S: StateReader>(
    HintArgs { hint_processor, vm, ap_tracking, ids_data, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    let account_tx = hint_processor
        .execution_helpers_manager
        .get_mut_current_execution_helper()?
        .tx_tracker
        .get_account_tx()?;

    // A declare transaction is expected.
    let AccountTransaction::Declare(declare_tx) = account_tx else {
        return Err(OsHintError::UnexpectedTxType(account_tx.tx_type()));
    };
    if declare_tx.version() != TransactionVersion::THREE {
        return Err(OsHintError::AssertionFailed {
            message: format!("Unsupported declare version: {:?}.", declare_tx.version()),
        });
    }
    insert_value_from_var_name(
        Ids::SenderAddress.into(),
        declare_tx.sender_address().0.key(),
        vm,
        ids_data,
        ap_tracking,
    )?;
    let account_deployment_data: Vec<_> = get_account_deployment_data(
        hint_processor.execution_helpers_manager.get_current_execution_helper()?,
    )?
    .0
    .iter()
    .map(MaybeRelocatable::from)
    .collect();

    insert_value_from_var_name(
        Ids::AccountDeploymentDataSize.into(),
        account_deployment_data.len(),
        vm,
        ids_data,
        ap_tracking,
    )?;
    let account_deployment_data_base = vm.gen_arg(&account_deployment_data)?;
    insert_value_from_var_name(
        Ids::AccountDeploymentData.into(),
        account_deployment_data_base,
        vm,
        ids_data,
        ap_tracking,
    )?;
    let class_hash_base = vm.gen_arg(&vec![MaybeRelocatable::from(declare_tx.class_hash().0)])?;
    insert_value_from_var_name(
        Ids::ClassHashPtr.into(),
        class_hash_base,
        vm,
        ids_data,
        ap_tracking,
    )?;
    insert_value_from_var_name(
        Ids::CompiledClassHash.into(),
        declare_tx.compiled_class_hash().0,
        vm,
        ids_data,
        ap_tracking,
    )?;

    Ok(())
}

#[allow(clippy::result_large_err)]
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

#[allow(clippy::result_large_err)]
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

#[allow(clippy::result_large_err)]
pub(crate) fn cache_contract_storage_request_key<S: StateReader>(
    hint_args: HintArgs<'_, '_, S>,
) -> OsHintResult {
    assert_value_cached_by_reading(
        hint_args,
        Ids::Request,
        CairoStruct::StorageReadRequest,
        &["key"],
    )
}

#[allow(clippy::result_large_err)]
pub(crate) fn cache_contract_storage_syscall_request_address<S: StateReader>(
    hint_args: HintArgs<'_, '_, S>,
) -> OsHintResult {
    assert_value_cached_by_reading(
        hint_args,
        Ids::SyscallPtr,
        CairoStruct::StorageReadPtr,
        &["request", "address"],
    )
}

#[allow(clippy::result_large_err)]
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

#[allow(clippy::result_large_err)]
pub(crate) fn fetch_result<S: StateReader>(
    HintArgs { vm, ids_data, ap_tracking, constants, .. }: HintArgs<'_, '_, S>,
) -> OsHintResult {
    // Fetch the result, up to 100 elements.
    let retdata = get_ptr_from_var_name(Ids::Retdata.into(), vm, ids_data, ap_tracking)?;
    let retdata_size = felt_to_usize(&get_integer_from_var_name(
        Ids::RetdataSize.into(),
        vm,
        ids_data,
        ap_tracking,
    )?)?;
    let result = vm.get_range(retdata, min(retdata_size, 100_usize));

    let validated = MaybeRelocatable::from(Const::Validated.fetch(constants)?);

    if retdata_size != 1 || result[0] != Some(Cow::Borrowed(&validated)) {
        log::info!("Invalid return value from __validate__:");
        log::info!("  Size: {retdata_size}");
        log::info!("  Result (at most 100 elements): {:?}", result);
    }
    Ok(())
}
