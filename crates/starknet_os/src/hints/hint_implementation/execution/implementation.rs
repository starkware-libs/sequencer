use std::borrow::Cow;
use std::cmp::min;
use std::collections::HashMap;

use blockifier::execution::contract_class::TrackedResource;
use blockifier::state::state_api::{State, StateReader};
use cairo_vm::any_box;
use cairo_vm::hint_processor::hint_processor_utils::felt_to_usize;
use cairo_vm::types::relocatable::MaybeRelocatable;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ClassHash, ContractAddress, Nonce, PatriciaKey};
use starknet_api::executable_transaction::{AccountTransaction, Transaction};
use starknet_api::state::StorageKey;
use starknet_api::transaction::fields::ValidResourceBounds;
use starknet_api::transaction::{DeployAccountTransaction, TransactionVersion};
use starknet_types_core::felt::Felt;

use crate::hint_processor::snos_hint_processor::SnosHintProcessor;
use crate::hints::error::{InnerInconsistentStorageValueError, OsHintError, OsHintResult};
use crate::hints::hint_implementation::execution::utils::{
    assert_retdata_as_expected,
    compare_retdata,
    extract_actual_retdata,
    get_account_deployment_data,
    get_calldata,
    get_proof_facts,
    set_state_entry,
};
use crate::hints::types::HintContext;
use crate::hints::vars::{CairoStruct, Const, Ids, Scope};
use crate::syscall_handler_utils::SyscallHandlerType;
use crate::vm_utils::{get_address_of_nested_fields_from_base_address, LoadCairoObject};

pub(crate) fn load_next_tx<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let execution_helper =
        hint_processor.execution_helpers_manager.get_mut_current_execution_helper()?;
    let tx = execution_helper.tx_tracker.load_next_tx()?;
    ctx.insert_value(Ids::TxType, tx.tx_type().tx_type_as_felt())?;

    // Log enter tx.
    let range_check_ptr = ctx.get_ptr(Ids::RangeCheckPtr)?;
    execution_helper.os_logger.enter_tx(
        tx.tx_type(),
        tx.tx_hash(),
        ctx.vm.get_current_step(),
        range_check_ptr,
        &ctx,
    )?;

    Ok(())
}

pub(crate) fn load_common_tx_fields<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let account_tx = hint_processor
        .execution_helpers_manager
        .get_current_execution_helper()?
        .tx_tracker
        .get_account_tx()?;

    // Guess the values.
    let tip = Felt::from(account_tx.tip().0);
    let paymaster_data_len = Felt::from(account_tx.paymaster_data().0.len());
    let paymaster_data: Vec<_> =
        account_tx.paymaster_data().0.into_iter().map(MaybeRelocatable::from).collect();
    let paymaster_data_base = ctx.vm.gen_arg(&paymaster_data)?;
    let nonce_da_mode_as_felt = Felt::from(account_tx.nonce_data_availability_mode());
    let fee_da_mode_as_felt = Felt::from(account_tx.fee_data_availability_mode());

    let resource_bounds = account_tx.resource_bounds();
    if let ValidResourceBounds::L1Gas(_) = resource_bounds {
        return Err(OsHintError::AssertionFailed {
            message: "Only transactions with 3 resource bounds are supported. Got 1 resource \
                      bounds."
                .to_string(),
        });
    }
    let resource_bound_address = ctx.vm.add_memory_segment();
    resource_bounds.load_into(
        ctx.vm,
        ctx.program,
        resource_bound_address,
        &ctx.program.constants,
    )?;

    // Insert.
    ctx.insert_value(Ids::Tip, tip)?;
    ctx.insert_value(Ids::PaymasterDataLength, paymaster_data_len)?;
    ctx.insert_value(Ids::PaymasterData, paymaster_data_base)?;
    ctx.insert_value(Ids::NonceDataAvailabilityMode, nonce_da_mode_as_felt)?;
    ctx.insert_value(Ids::FeeDataAvailabilityMode, fee_da_mode_as_felt)?;
    ctx.insert_value(Ids::ResourceBounds, resource_bound_address)?;
    Ok(())
}

pub(crate) fn exit_tx<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    let range_check_ptr = ctx.get_ptr(Ids::RangeCheckPtr)?;
    Ok(hint_processor
        .execution_helpers_manager
        .get_mut_current_execution_helper()?
        .os_logger
        .exit_tx(ctx.vm.get_current_step(), range_check_ptr, &ctx)?)
}

pub(crate) fn prepare_constructor_execution<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let account_tx = hint_processor
        .execution_helpers_manager
        .get_mut_current_execution_helper()?
        .tx_tracker
        .get_account_tx()?;
    let AccountTransaction::DeployAccount(deploy_account_tx) = account_tx else {
        return Err(OsHintError::UnexpectedTxType(account_tx.tx_type()));
    };

    ctx.insert_value(Ids::ContractAddressSalt, deploy_account_tx.contract_address_salt().0)?;
    ctx.insert_value(Ids::ClassHash, deploy_account_tx.class_hash().0)?;

    let constructor_calldata = match &deploy_account_tx.tx {
        DeployAccountTransaction::V1(v1_tx) => &v1_tx.constructor_calldata,
        DeployAccountTransaction::V3(v3_tx) => &v3_tx.constructor_calldata,
    };
    ctx.insert_value(Ids::ConstructorCalldataSize, constructor_calldata.0.len())?;
    let constructor_calldata_base = ctx.vm.add_memory_segment();
    let constructor_calldata_as_relocatable: Vec<MaybeRelocatable> =
        constructor_calldata.0.iter().map(MaybeRelocatable::from).collect();
    ctx.vm.load_data(constructor_calldata_base, &constructor_calldata_as_relocatable)?;
    ctx.insert_value(Ids::ConstructorCalldata, constructor_calldata_base)?;
    Ok(())
}

pub(crate) fn assert_transaction_hash<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    let stored_transaction_hash = ctx.get_integer(Ids::TransactionHash)?;
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

pub(crate) fn enter_scope_deprecated_syscall_handler(ctx: HintContext<'_>) -> OsHintResult {
    let new_scope = HashMap::from([(
        Scope::SyscallHandlerType.into(),
        any_box!(SyscallHandlerType::DeprecatedSyscallHandler),
    )]);
    ctx.exec_scopes.enter_scope(new_scope);
    Ok(())
}

pub(crate) fn enter_scope_syscall_handler(ctx: HintContext<'_>) -> OsHintResult {
    let new_scope = HashMap::from([(
        Scope::SyscallHandlerType.into(),
        any_box!(SyscallHandlerType::SyscallHandler),
    )]);
    ctx.exec_scopes.enter_scope(new_scope);
    Ok(())
}

pub(crate) fn get_contract_address_state_entry(mut ctx: HintContext<'_>) -> OsHintResult {
    let contract_address = ctx.get_integer(Ids::ContractAddress)?;
    set_state_entry(&contract_address, &mut ctx)?;
    Ok(())
}

pub(crate) fn set_state_entry_to_account_contract_address<S: StateReader>(
    _hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let account_contract_address = ctx.get_nested_field_felt(
        Ids::TxInfo,
        CairoStruct::TxInfoPtr,
        &["account_contract_address"],
    )?;
    set_state_entry(&account_contract_address, &mut ctx)?;
    Ok(())
}

pub(crate) fn check_is_deprecated<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let class_hash = ClassHash(ctx.get_nested_field_felt(
        Ids::ExecutionContext,
        CairoStruct::ExecutionContextPtr,
        &["class_hash"],
    )?);

    let is_deprecated = Felt::from(hint_processor.deprecated_class_hashes.contains(&class_hash));
    ctx.insert_value(Ids::IsDeprecated, is_deprecated)?;

    Ok(())
}

pub(crate) fn enter_scope_execute_transactions_inner(ctx: HintContext<'_>) -> OsHintResult {
    // Unlike the Python implementation, there is no need to add `syscall_handler`,
    // `deprecated_syscall_handler`, `deprecated_class_hashes` and `execution_helper` as scope
    // variables since they are accessible via the hint processor.
    let dict_manager = ctx.get_dict_manager()?;

    let new_scope = HashMap::from([(Scope::DictManager.into(), any_box!(dict_manager))]);
    ctx.exec_scopes.enter_scope(new_scope);

    Ok(())
}

pub(crate) fn end_tx<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    _ctx: HintContext<'_>,
) -> OsHintResult {
    hint_processor.get_mut_current_execution_helper()?.tx_execution_iter.end_tx()?;
    Ok(())
}

pub(crate) fn enter_call<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    let execution_info_ptr = ctx.get_nested_field_ptr(
        Ids::ExecutionContext,
        CairoStruct::ExecutionContextPtr,
        &["execution_info"],
    )?;
    let deprecated_tx_info_ptr = ctx.get_nested_field_ptr(
        Ids::ExecutionContext,
        CairoStruct::ExecutionContextPtr,
        &["deprecated_tx_info"],
    )?;

    hint_processor
        .get_mut_current_execution_helper()?
        .tx_execution_iter
        .get_mut_tx_execution_info_ref()?
        .enter_call(execution_info_ptr, deprecated_tx_info_ptr)?;
    Ok(())
}

pub(crate) fn exit_call<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    _ctx: HintContext<'_>,
) -> OsHintResult {
    hint_processor
        .get_mut_current_execution_helper()?
        .tx_execution_iter
        .get_mut_tx_execution_info_ref()?
        .exit_call_info()?;
    Ok(())
}

pub(crate) fn contract_address<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let tx = hint_processor.get_current_execution_helper()?.tx_tracker.get_tx()?;
    let contract_address = match tx {
        Transaction::Account(account_tx) => account_tx.sender_address(),
        Transaction::L1Handler(l1_handler) => l1_handler.tx.contract_address,
    };
    ctx.insert_value(Ids::ContractAddress, **contract_address)?;
    Ok(())
}

pub(crate) fn tx_calldata<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let calldata: Vec<_> =
        get_calldata(hint_processor.execution_helpers_manager.get_current_execution_helper()?)?
            .0
            .iter()
            .map(MaybeRelocatable::from)
            .collect();
    let calldata_base = ctx.vm.gen_arg(&calldata)?;
    ctx.insert_value(Ids::Calldata, calldata_base)?;
    ctx.insert_value(Ids::CalldataSize, Felt::from(calldata.len()))?;
    Ok(())
}

pub(crate) fn tx_entry_point_selector<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
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
    ctx.insert_value(Ids::EntryPointSelector, entry_point_selector.0)?;
    Ok(())
}

pub(crate) fn tx_version<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let version = hint_processor.get_current_execution_helper()?.tx_tracker.get_tx()?.version();
    ctx.insert_value(Ids::TxVersion, version.0)?;
    Ok(())
}

pub(crate) fn tx_account_deployment_data<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let account_deployment_data: Vec<_> =
        get_account_deployment_data(hint_processor.get_current_execution_helper()?)?
            .0
            .iter()
            .map(MaybeRelocatable::from)
            .collect();
    let account_deployment_data_base = ctx.vm.gen_arg(&account_deployment_data)?;
    ctx.insert_value(Ids::AccountDeploymentData, account_deployment_data_base)?;
    ctx.insert_value(Ids::AccountDeploymentDataSize, Felt::from(account_deployment_data.len()))?;
    Ok(())
}

pub(crate) fn tx_proof_facts<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let proof_facts: Vec<_> = get_proof_facts(hint_processor.get_current_execution_helper()?)?
        .0
        .iter()
        .map(MaybeRelocatable::from)
        .collect();
    let proof_facts_base = ctx.vm.gen_arg(&proof_facts)?;
    ctx.insert_value(Ids::ProofFacts, proof_facts_base)?;
    ctx.insert_value(Ids::ProofFactsSize, proof_facts.len())?;
    Ok(())
}

pub(crate) fn gen_signature_arg<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let account_tx = hint_processor
        .execution_helpers_manager
        .get_current_execution_helper()?
        .tx_tracker
        .get_account_tx()?;
    let signature: Vec<_> = account_tx.signature().0.iter().map(MaybeRelocatable::from).collect();
    let signature_start = ctx.vm.gen_arg(&signature)?;
    ctx.insert_value(Ids::SignatureStart, signature_start)?;
    ctx.insert_value(Ids::SignatureLen, signature.len())?;

    Ok(())
}

pub(crate) fn is_reverted<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let is_reverted = hint_processor
        .execution_helpers_manager
        .get_current_execution_helper()?
        .tx_execution_iter
        .get_tx_execution_info_ref()?
        .tx_execution_info
        .is_reverted();
    ctx.insert_value(Ids::IsReverted, Felt::from(is_reverted))?;
    Ok(())
}

pub(crate) fn check_execution_and_exit_call<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    let current_execution_helper =
        hint_processor.execution_helpers_manager.get_mut_current_execution_helper()?;
    if current_execution_helper.os_logger.debug {
        // Validate the predicted gas cost.
        // TODO(Yoni): remove this check once Cairo 0 is not supported.
        let remaining_gas = ctx.get_integer(Ids::RemainingGas)?;
        let gas_builtin = ctx.get_nested_field_felt(
            Ids::EntryPointReturnValues,
            CairoStruct::EntryPointReturnValuesPtr,
            &["gas_builtin"],
        )?;
        let actual_gas = remaining_gas - gas_builtin;

        let call_info = current_execution_helper
            .tx_execution_iter
            .get_tx_execution_info_ref()?
            .get_call_info_tracker()?
            .call_info;
        let mut predicted = Felt::from(call_info.execution.gas_consumed);

        match call_info.tracked_resource {
            TrackedResource::SierraGas => {
                let initial_budget = ctx.fetch_const(Const::EntryPointInitialBudget)?;
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

    let syscall_ptr_end = ctx.get_nested_field_ptr(
        Ids::EntryPointReturnValues,
        CairoStruct::EntryPointReturnValuesPtr,
        &["syscall_ptr"],
    )?;
    current_execution_helper
        .syscall_hint_processor
        .validate_and_discard_syscall_ptr(&syscall_ptr_end)?;
    current_execution_helper.tx_execution_iter.get_mut_tx_execution_info_ref()?.exit_call_info()?;
    Ok(())
}

pub(crate) fn is_remaining_gas_lt_initial_budget(mut ctx: HintContext<'_>) -> OsHintResult {
    let remaining_gas = ctx.get_integer(Ids::RemainingGas)?;
    let initial_budget = ctx.fetch_const(Const::EntryPointInitialBudget)?;
    let remaining_gas_lt_initial_budget: Felt = (&remaining_gas < initial_budget).into();
    Ok(ctx.insert_value(Ids::IsRemainingGasLtInitialBudget, remaining_gas_lt_initial_budget)?)
}

pub(crate) fn check_syscall_response<S: StateReader>(
    _hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    let actual_retdata = extract_actual_retdata(ctx.vm, ctx.ids_data, ctx.ap_tracking)?;
    let call_response_ptr = ctx.get_ptr(Ids::CallResponse)?;
    let retdata_size = ctx.vm.get_integer(get_address_of_nested_fields_from_base_address(
        call_response_ptr,
        CairoStruct::DeprecatedCallContractResponse,
        ctx.vm,
        &["retdata_size"],
        ctx.program,
    )?)?;
    let retdata_base = ctx.vm.get_relocatable(get_address_of_nested_fields_from_base_address(
        call_response_ptr,
        CairoStruct::DeprecatedCallContractResponse,
        ctx.vm,
        &["retdata"],
        ctx.program,
    )?)?;
    let expected_retdata =
        ctx.vm.get_continuous_range(retdata_base, felt_to_usize(&retdata_size)?)?;
    compare_retdata(&actual_retdata, &expected_retdata)
}

pub(crate) fn check_new_call_contract_response<S: StateReader>(
    _hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    assert_retdata_as_expected(
        "retdata_start",
        "retdata_end",
        CairoStruct::CallContractResponse,
        ctx.vm,
        ctx.ap_tracking,
        ctx.ids_data,
        ctx.program,
    )
}

pub(crate) fn check_new_deploy_response<S: StateReader>(
    _hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    assert_retdata_as_expected(
        "constructor_retdata_start",
        "constructor_retdata_end",
        CairoStruct::DeployResponse,
        ctx.vm,
        ctx.ap_tracking,
        ctx.ids_data,
        ctx.program,
    )
}

pub(crate) fn initial_ge_required_gas(mut ctx: HintContext<'_>) -> OsHintResult {
    let initial_gas = ctx.get_integer(Ids::InitialGas)?;
    let required_gas = ctx.get_integer(Ids::RequiredGas)?;
    ctx.insert_value(Ids::InitialGeRequiredGas, Felt::from(initial_gas >= required_gas))?;
    Ok(())
}

fn load_tx_nonce(nonce: Nonce, mut ctx: HintContext<'_>) -> OsHintResult {
    ctx.insert_value(Ids::Nonce, nonce.0)?;
    Ok(())
}

pub(crate) fn load_tx_nonce_account<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    let nonce = hint_processor
        .execution_helpers_manager
        .get_current_execution_helper()?
        .tx_tracker
        .get_account_tx()?
        .nonce();
    load_tx_nonce(nonce, ctx)
}

pub(crate) fn load_tx_nonce_l1_handler<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    let nonce = hint_processor
        .execution_helpers_manager
        .get_current_execution_helper()?
        .tx_tracker
        .get_tx()?
        .nonce();
    load_tx_nonce(nonce, ctx)
}

fn write_syscall_result_helper<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
    ids_type: Ids,
    struct_type: CairoStruct,
    key_name: &str,
) -> OsHintResult {
    let key_felt = ctx.get_nested_field_felt(ids_type, struct_type, &[key_name])?;
    let key = StorageKey(PatriciaKey::try_from(key_felt)?);

    let contract_address = ContractAddress(ctx.get_integer(Ids::ContractAddress)?.try_into()?);

    let current_execution_helper =
        hint_processor.execution_helpers_manager.get_mut_current_execution_helper()?;
    let prev_value = current_execution_helper.cached_state.get_storage_at(contract_address, key)?;

    ctx.insert_value(Ids::PrevValue, prev_value)?;

    let request_value = ctx.get_nested_field_felt(ids_type, struct_type, &["value"])?;

    current_execution_helper.cached_state.set_storage_at(contract_address, key, request_value)?;

    set_state_entry(contract_address.key(), &mut ctx)
}

pub(crate) fn write_syscall_result_deprecated<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    write_syscall_result_helper(
        hint_processor,
        ctx,
        Ids::SyscallPtr,
        CairoStruct::StorageWritePtr,
        "address",
    )
}

pub(crate) fn write_syscall_result<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    write_syscall_result_helper(
        hint_processor,
        ctx,
        Ids::Request,
        CairoStruct::StorageWriteRequest,
        "key",
    )
}

pub(crate) fn declare_tx_fields<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
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
    ctx.insert_value(Ids::SenderAddress, declare_tx.sender_address().0.key())?;
    let account_deployment_data: Vec<_> = get_account_deployment_data(
        hint_processor.execution_helpers_manager.get_current_execution_helper()?,
    )?
    .0
    .iter()
    .map(MaybeRelocatable::from)
    .collect();

    ctx.insert_value(Ids::AccountDeploymentDataSize, account_deployment_data.len())?;
    let account_deployment_data_base = ctx.vm.gen_arg(&account_deployment_data)?;
    ctx.insert_value(Ids::AccountDeploymentData, account_deployment_data_base)?;
    let class_hash_base =
        ctx.vm.gen_arg(&vec![MaybeRelocatable::from(declare_tx.class_hash().0)])?;
    ctx.insert_value(Ids::ClassHashPtr, class_hash_base)?;
    ctx.insert_value(Ids::CompiledClassHash, declare_tx.compiled_class_hash().0)?;

    Ok(())
}

pub(crate) fn write_old_block_to_storage<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    let execution_helper = &mut hint_processor.get_mut_current_execution_helper()?;

    let block_hash_contract_address = ctx.fetch_const(Const::BlockHashContractAddress)?;
    let old_block_number = ctx.get_integer(Ids::OldBlockNumber)?;
    let old_block_hash = ctx.get_integer(Ids::OldBlockHash)?;

    log::debug!("writing block number: {old_block_number} -> block hash: {old_block_hash}");

    execution_helper.cached_state.set_storage_at(
        ContractAddress(PatriciaKey::try_from(*block_hash_contract_address)?),
        StorageKey(PatriciaKey::try_from(old_block_number)?),
        old_block_hash,
    )?;
    Ok(())
}

fn assert_value_cached_by_reading<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
    id: Ids,
    cairo_struct_type: CairoStruct,
    nested_fields: &[&str],
) -> OsHintResult {
    let key_felt = ctx.get_nested_field_felt(id, cairo_struct_type, nested_fields)?;
    let key = StorageKey(PatriciaKey::try_from(key_felt)?);

    let contract_address = ContractAddress(ctx.get_integer(Ids::ContractAddress)?.try_into()?);

    let value = hint_processor
        .get_current_execution_helper()?
        .cached_state
        .get_storage_at(contract_address, key)?;

    let ids_value = ctx.get_integer(Ids::Value)?;

    if value != ids_value {
        return Err(OsHintError::InconsistentStorageValue(Box::new(
            InnerInconsistentStorageValueError {
                contract_address,
                key,
                expected: value,
                actual: ids_value,
            },
        )));
    }
    Ok(())
}

pub(crate) fn cache_contract_storage_request_key<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    assert_value_cached_by_reading(
        hint_processor,
        ctx,
        Ids::Request,
        CairoStruct::StorageReadRequest,
        &["key"],
    )
}

pub(crate) fn cache_contract_storage_syscall_request_address<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    ctx: HintContext<'_>,
) -> OsHintResult {
    assert_value_cached_by_reading(
        hint_processor,
        ctx,
        Ids::SyscallPtr,
        CairoStruct::StorageReadPtr,
        &["request", "address"],
    )
}

pub(crate) fn get_old_block_number_and_hash<S: StateReader>(
    hint_processor: &mut SnosHintProcessor<'_, S>,
    mut ctx: HintContext<'_>,
) -> OsHintResult {
    let os_input = &hint_processor.get_current_execution_helper()?.os_block_input;
    let (old_block_number, old_block_hash) =
        os_input.old_block_number_and_hash.ok_or(OsHintError::BlockNumberTooSmall {
            stored_block_hash_buffer: *ctx.fetch_const(Const::StoredBlockHashBuffer)?,
        })?;

    let ids_old_block_number = BlockNumber(
        ctx.get_integer(Ids::OldBlockNumber)?.try_into().expect("Block number should fit in u64"),
    );
    if old_block_number != ids_old_block_number {
        return Err(OsHintError::InconsistentBlockNumber {
            expected: old_block_number,
            actual: ids_old_block_number,
        });
    }
    ctx.insert_value(Ids::OldBlockHash, old_block_hash.0)?;
    Ok(())
}

pub(crate) fn check_retdata_for_debug(ctx: HintContext<'_>) -> OsHintResult {
    // Fetch the result, up to 100 elements.
    let retdata = ctx.get_ptr(Ids::Retdata)?;
    let retdata_size = felt_to_usize(&ctx.get_integer(Ids::RetdataSize)?)?;
    let result = ctx.vm.get_range(retdata, min(retdata_size, 100_usize));

    let validated_value = MaybeRelocatable::from(ctx.fetch_const(Const::Validated)?);

    if retdata_size != 1 || result[0] != Some(Cow::Borrowed(&validated_value)) {
        log::info!("Invalid return value from __validate__:");
        log::info!("  Size: {retdata_size}");
        log::info!("  Result (at most 100 elements): {result:?}");
    }
    Ok(())
}
