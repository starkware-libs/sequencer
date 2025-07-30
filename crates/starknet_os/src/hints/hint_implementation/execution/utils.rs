use std::collections::HashMap;

use blockifier::state::state_api::StateReader;
use cairo_vm::hint_processor::builtin_hint_processor::hint_utils::{
    get_integer_from_var_name,
    get_ptr_from_var_name,
    insert_value_from_var_name,
};
use cairo_vm::hint_processor::hint_processor_definition::HintReference;
use cairo_vm::hint_processor::hint_processor_utils::felt_to_usize;
use cairo_vm::serde::deserialize_program::ApTracking;
use cairo_vm::types::exec_scope::ExecutionScopes;
use cairo_vm::types::relocatable::{MaybeRelocatable, Relocatable};
use cairo_vm::vm::vm_core::VirtualMachine;
use starknet_api::executable_transaction::{AccountTransaction, Transaction};
use starknet_api::transaction::fields::{
    valid_resource_bounds_as_felts,
    AccountDeploymentData,
    Calldata,
    ResourceAsFelts,
    ValidResourceBounds,
};
use starknet_api::transaction::InvokeTransaction;
use starknet_types_core::felt::Felt;

use crate::hint_processor::execution_helper::OsExecutionHelper;
use crate::hints::error::{OsHintError, OsHintResult};
use crate::hints::vars::{CairoStruct, Ids};
use crate::vm_utils::{
    get_address_of_nested_fields_from_base_address,
    insert_values_to_fields,
    CairoSized,
    IdentifierGetter,
    LoadCairoObject,
    VmUtilsError,
    VmUtilsResult,
};

/// Corresponds to the ResourceBounds struct in Cairo.
impl<IG: IdentifierGetter> LoadCairoObject<IG> for ResourceAsFelts {
    fn load_into(
        &self,
        vm: &mut VirtualMachine,
        identifier_getter: &IG,
        address: Relocatable,
        _constants: &HashMap<String, Felt>,
    ) -> VmUtilsResult<Relocatable> {
        let resource_bounds_list = vec![
            ("resource", self.resource_name.into()),
            ("max_amount", self.max_amount.into()),
            ("max_price_per_unit", self.max_price_per_unit.into()),
        ];
        insert_values_to_fields(
            address,
            CairoStruct::ResourceBounds,
            vm,
            &resource_bounds_list,
            identifier_getter,
        )?;
        Ok((address + Self::size(identifier_getter)?)?)
    }
}

impl<IG: IdentifierGetter> CairoSized<IG> for ResourceAsFelts {
    fn cairo_struct() -> CairoStruct {
        CairoStruct::ResourceBounds
    }
}

/// Corresponds to the ResourceBounds struct in Cairo.
impl<IG: IdentifierGetter> LoadCairoObject<IG> for ValidResourceBounds {
    fn load_into(
        &self,
        vm: &mut VirtualMachine,
        identifier_getter: &IG,
        address: Relocatable,
        constants: &HashMap<String, Felt>,
    ) -> VmUtilsResult<Relocatable> {
        valid_resource_bounds_as_felts(self, false)
            .map_err(VmUtilsError::ResourceBoundsParsing)?
            .load_into(vm, identifier_getter, address, constants)
    }
}

pub(crate) fn get_account_deployment_data<S: StateReader>(
    execution_helper: &OsExecutionHelper<'_, S>,
) -> Result<AccountDeploymentData, OsHintError> {
    let tx = execution_helper.tx_tracker.get_account_tx()?;
    match tx {
        AccountTransaction::Declare(declare) => Ok(declare.account_deployment_data()),
        AccountTransaction::Invoke(invoke) => Ok(invoke.account_deployment_data()),
        AccountTransaction::DeployAccount(_) => Err(OsHintError::UnexpectedTxType(tx.tx_type())),
    }
}

pub(crate) fn get_calldata<'a, S: StateReader>(
    execution_helper: &OsExecutionHelper<'a, S>,
) -> Result<&'a Calldata, OsHintError> {
    let tx = execution_helper.tx_tracker.get_tx()?;
    match tx {
        Transaction::L1Handler(l1_handler) => Ok(&l1_handler.tx.calldata),
        Transaction::Account(AccountTransaction::Invoke(invoke)) => Ok(match &invoke.tx {
            InvokeTransaction::V0(invoke_tx_v0) => &invoke_tx_v0.calldata,
            InvokeTransaction::V1(invoke_tx_v1) => &invoke_tx_v1.calldata,
            InvokeTransaction::V3(invoke_tx_v3) => &invoke_tx_v3.calldata,
        }),
        _ => Err(OsHintError::UnexpectedTxType(tx.tx_type())),
    }
}

pub(crate) fn set_state_entry<'a>(
    key: &Felt,
    vm: &'a mut VirtualMachine,
    exec_scopes: &'a mut ExecutionScopes,
    ids_data: &'a HashMap<String, HintReference>,
    ap_tracking: &'a ApTracking,
) -> OsHintResult {
    let state_changes_ptr =
        get_ptr_from_var_name(Ids::ContractStateChanges.into(), vm, ids_data, ap_tracking)?;
    let dict_manager = exec_scopes.get_dict_manager()?;
    let mut dict_manager_borrowed = dict_manager.borrow_mut();
    let state_entry =
        dict_manager_borrowed.get_tracker_mut(state_changes_ptr)?.get_value(&key.into())?;
    insert_value_from_var_name(Ids::StateEntry.into(), state_entry, vm, ids_data, ap_tracking)?;
    Ok(())
}

pub(crate) fn assert_retdata_as_expected<IG: IdentifierGetter>(
    retdata_start_field_name: &str,
    retdata_end_field_name: &str,
    response_type: CairoStruct,
    vm: &VirtualMachine,
    ap_tracking: &ApTracking,
    ids_data: &HashMap<String, HintReference>,
    identifier_getter: &IG,
) -> OsHintResult {
    let response_ptr = get_ptr_from_var_name(Ids::Response.into(), vm, ids_data, ap_tracking)?;
    let response_start = vm.get_relocatable(get_address_of_nested_fields_from_base_address(
        response_ptr,
        response_type,
        vm,
        &[retdata_start_field_name],
        identifier_getter,
    )?)?;

    let response_end = vm.get_relocatable(get_address_of_nested_fields_from_base_address(
        response_ptr,
        response_type,
        vm,
        &[retdata_end_field_name],
        identifier_getter,
    )?)?;

    let response_len = (response_end - response_start)?;
    let expected_retdata = vm.get_continuous_range(response_start, response_len)?;
    let actual_retdata = extract_actual_retdata(vm, ids_data, ap_tracking)?;
    compare_retdata(&actual_retdata, &expected_retdata)
}

pub(crate) fn extract_actual_retdata(
    vm: &VirtualMachine,
    ids_data: &HashMap<String, HintReference>,
    ap_tracking: &ApTracking,
) -> Result<Vec<MaybeRelocatable>, OsHintError> {
    let retdata_size = felt_to_usize(&get_integer_from_var_name(
        Ids::RetdataSize.into(),
        vm,
        ids_data,
        ap_tracking,
    )?)?;
    if retdata_size == 0 {
        // Note that retdata type is not defined if retdata_size is 0.
        return Ok(vec![]);
    }
    let retdata_base = get_ptr_from_var_name(Ids::Retdata.into(), vm, ids_data, ap_tracking)?;
    Ok(vm.get_continuous_range(retdata_base, retdata_size)?)
}

pub(crate) fn compare_retdata(
    actual_retdata: &Vec<MaybeRelocatable>,
    expected_retdata: &Vec<MaybeRelocatable>,
) -> OsHintResult {
    if actual_retdata != expected_retdata {
        return Err(OsHintError::AssertionFailed {
            message: format!(
                "Return value mismatch; expected={expected_retdata:?}, actual={actual_retdata:?}."
            ),
        });
    }
    Ok(())
}
