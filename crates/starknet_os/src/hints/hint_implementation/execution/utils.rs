use std::collections::HashMap;

use blockifier::state::state_api::StateReader;
use cairo_vm::types::relocatable::Relocatable;
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
use crate::hints::error::OsHintError;
use crate::hints::vars::CairoStruct;
use crate::vm_utils::{
    insert_values_to_fields,
    CairoSized,
    IdentifierGetter,
    LoadCairoObject,
    VmUtilsError,
    VmUtilsResult,
};

impl<IG: IdentifierGetter> LoadCairoObject<IG> for ResourceAsFelts {
    fn load_into(
        &self,
        vm: &mut VirtualMachine,
        identifier_getter: &IG,
        address: Relocatable,
        _constants: &HashMap<String, Felt>,
    ) -> VmUtilsResult<()> {
        let resource_bounds_list = vec![
            ("resource_name", self.resource_name.into()),
            ("max_amount", self.max_amount.into()),
            ("max_price_per_unit", self.max_price_per_unit.into()),
        ];
        insert_values_to_fields(
            address,
            CairoStruct::ResourceBounds,
            vm,
            &resource_bounds_list,
            identifier_getter,
        )
    }
}

impl<IG: IdentifierGetter> CairoSized<IG> for ResourceAsFelts {
    fn size(_identifier_getter: &IG) -> usize {
        3
    }
}

impl<IG: IdentifierGetter> LoadCairoObject<IG> for ValidResourceBounds {
    fn load_into(
        &self,
        vm: &mut VirtualMachine,
        identifier_getter: &IG,
        address: Relocatable,
        constants: &HashMap<String, Felt>,
    ) -> VmUtilsResult<()> {
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
