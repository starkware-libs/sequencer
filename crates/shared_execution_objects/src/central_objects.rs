use std::collections::HashMap;

use blockifier::abi::constants as abi_constants;
use blockifier::execution::call_info::{CallInfo, CallInfoIter};
use blockifier::fee::receipt::TransactionReceipt;
use blockifier::transaction::objects::{ExecutionResourcesTraits, TransactionExecutionInfo};
use serde::Serialize;
use starknet_api::executable_transaction::TransactionType;
use starknet_api::execution_resources::GasVector;
use starknet_api::transaction::fields::Fee;

/// A mapping from a transaction execution resource to its actual usage.
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct ResourcesMapping(pub HashMap<String, usize>);

impl From<TransactionReceipt> for ResourcesMapping {
    fn from(receipt: TransactionReceipt) -> ResourcesMapping {
        let vm_resources = &receipt.resources.computation.total_vm_resources();
        let mut resources = HashMap::from([(
            abi_constants::N_STEPS_RESOURCE.to_string(),
            vm_resources.total_n_steps() + receipt.resources.computation.n_reverted_steps,
        )]);
        resources.extend(
            vm_resources
                .prover_builtins()
                .iter()
                .map(|(builtin, value)| (builtin.to_str_with_suffix().to_string(), *value)),
        );

        ResourcesMapping(resources)
    }
}

/// The TransactionExecutionInfo object as used by the Python code.
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
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
    fn from(tx_execution_info: TransactionExecutionInfo) -> CentralTransactionExecutionInfo {
        CentralTransactionExecutionInfo {
            validate_call_info: tx_execution_info.validate_call_info,
            execute_call_info: tx_execution_info.execute_call_info,
            fee_transfer_call_info: tx_execution_info.fee_transfer_call_info,
            actual_fee: tx_execution_info.receipt.fee,
            da_gas: tx_execution_info.receipt.da_gas,
            revert_error: tx_execution_info.revert_error.map(|error| error.to_string()),
            total_gas: tx_execution_info.receipt.gas,
            actual_resources: tx_execution_info.receipt.into(),
        }
    }
}

impl CentralTransactionExecutionInfo {
    pub fn call_info_iter(&self, tx_type: TransactionType) -> CallInfoIter<'_> {
        let ordered_call_infos = match tx_type {
            TransactionType::DeployAccount => {
                [&self.execute_call_info, &self.validate_call_info, &self.fee_transfer_call_info]
            }
            _ => [&self.validate_call_info, &self.execute_call_info, &self.fee_transfer_call_info],
        };
        CallInfoIter::new(ordered_call_infos.into_iter().filter_map(|call| call.as_ref()).collect())
    }

    pub fn is_reverted(&self) -> bool {
        self.revert_error.is_some()
    }
}
