use std::collections::HashMap;

use blockifier::abi::constants as abi_constants;
use blockifier::execution::call_info::{CairoPrimitiveName, CallInfo, CallInfoIter};
use blockifier::fee::receipt::TransactionReceipt;
use blockifier::transaction::objects::{ExecutionResourcesTraits, TransactionExecutionInfo};
use serde::Serialize;
use starknet_api::execution_resources::GasVector;
use starknet_api::transaction::constants::VALIDATE_DEPLOY_ENTRY_POINT_SELECTOR;
use starknet_api::transaction::fields::Fee;

/// A mapping from a transaction execution resource to its actual usage.
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct ResourcesMapping(pub HashMap<String, usize>);

impl From<TransactionReceipt> for ResourcesMapping {
    fn from(receipt: TransactionReceipt) -> ResourcesMapping {
        ResourcesMapping::from(&receipt)
    }
}

impl From<&TransactionReceipt> for ResourcesMapping {
    fn from(receipt: &TransactionReceipt) -> ResourcesMapping {
        let vm_resources = &receipt.resources.computation.total_extended_vm_resources();
        let mut resources = HashMap::from([(
            abi_constants::N_STEPS_RESOURCE.to_string(),
            vm_resources.vm_resources.total_n_steps()
                + receipt.resources.computation.n_reverted_steps,
        )]);
        resources.extend(vm_resources.prover_cairo_primitives().iter().map(
            |(primitive, value)| {
                let name = match primitive {
                    CairoPrimitiveName::Builtin(builtin) => {
                        builtin.to_str_with_suffix().to_string()
                    }
                    CairoPrimitiveName::Opcode(opcode) => opcode.to_str_with_suffix().to_string(),
                };
                (name, *value)
            },
        ));

        ResourcesMapping(resources)
    }
}

/// The TransactionExecutionInfo object as used by the Python code.
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize, Clone))]
#[derive(Debug, Serialize)]
pub struct CentralTransactionExecutionInfo {
    pub validate_call_info: Option<CallInfo>,
    pub execute_call_info: Option<CallInfo>,
    pub fee_transfer_call_info: Option<CallInfo>,
    pub actual_fee: Fee,
    pub da_gas: GasVector,
    pub actual_resources: ResourcesMapping,
    pub revert_error: Option<String>,
    pub total_gas: GasVector,
}

impl From<TransactionExecutionInfo> for CentralTransactionExecutionInfo {
    fn from(info: TransactionExecutionInfo) -> CentralTransactionExecutionInfo {
        CentralTransactionExecutionInfo {
            validate_call_info: info.validate_call_info,
            execute_call_info: info.execute_call_info,
            fee_transfer_call_info: info.fee_transfer_call_info,
            actual_fee: info.receipt.fee,
            da_gas: info.receipt.da_gas,
            revert_error: info.revert_error.map(|error| error.to_string()),
            total_gas: info.receipt.gas,
            actual_resources: info.receipt.into(),
        }
    }
}

impl From<&TransactionExecutionInfo> for CentralTransactionExecutionInfo {
    fn from(info: &TransactionExecutionInfo) -> CentralTransactionExecutionInfo {
        CentralTransactionExecutionInfo {
            validate_call_info: info.validate_call_info.clone(),
            execute_call_info: info.execute_call_info.clone(),
            fee_transfer_call_info: info.fee_transfer_call_info.clone(),
            actual_fee: info.receipt.fee,
            da_gas: info.receipt.da_gas,
            revert_error: info.revert_error.as_ref().map(|error| error.to_string()),
            total_gas: info.receipt.gas,
            actual_resources: ResourcesMapping::from(&info.receipt),
        }
    }
}

impl CentralTransactionExecutionInfo {
    pub fn call_info_iter(&self) -> CallInfoIter<'_> {
        let ordered_call_infos = if self.is_deploy_account() {
            [&self.execute_call_info, &self.validate_call_info, &self.fee_transfer_call_info]
        } else {
            [&self.validate_call_info, &self.execute_call_info, &self.fee_transfer_call_info]
        };
        CallInfoIter::new(ordered_call_infos.into_iter().filter_map(|call| call.as_ref()).collect())
    }

    fn is_deploy_account(&self) -> bool {
        if let Some(call_info) = self.validate_call_info.as_ref() {
            call_info.call.entry_point_selector == *VALIDATE_DEPLOY_ENTRY_POINT_SELECTOR
        } else {
            false
        }
    }

    pub fn is_reverted(&self) -> bool {
        self.revert_error.is_some()
    }
}
