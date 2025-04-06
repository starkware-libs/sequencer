use std::collections::HashMap;

use blockifier::abi::constants as abi_constants;
use blockifier::execution::call_info::CallInfo;
use blockifier::fee::receipt::TransactionReceipt;
use blockifier::transaction::objects::{ExecutionResourcesTraits, TransactionExecutionInfo};
use serde::Serialize;
use serde_json::json;
use starknet_api::execution_resources::GasVector;
use starknet_api::transaction::fields::Fee;

/// A mapping from a transaction execution resource to its actual usage.
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct ResourcesMapping(pub HashMap<String, usize>);

impl From<&TransactionReceipt> for ResourcesMapping {
    fn from(receipt: &TransactionReceipt) -> ResourcesMapping {
        let vm_resources = &receipt.resources.computation.vm_resources;
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
#[derive(Debug, Serialize, PartialEq)]
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
            actual_resources: (&tx_execution_info.receipt).into(),
        }
    }
}

/// Converts a TransactionExecutionInfo object to a JSON string that is compatible with the
/// CentralTransactionExecutionInfo.
pub fn execution_info_to_serialized_central_execution_info(
    tx_execution_info: &TransactionExecutionInfo,
) -> String {
    let receipt = &tx_execution_info.receipt;
    let vm_resources = &receipt.resources.computation.vm_resources;

    // Manually build the actual_resources mapping.
    let mut resources_map = serde_json::Map::new();
    resources_map.insert(
        abi_constants::N_STEPS_RESOURCE.to_string(),
        json!(vm_resources.total_n_steps() + receipt.resources.computation.n_reverted_steps),
    );
    for (builtin, value) in vm_resources.prover_builtins().iter() {
        resources_map.insert(builtin.to_str_with_suffix().to_string(), json!(value));
    }

    let central_json = json!({
        "validate_call_info": tx_execution_info.validate_call_info,
        "execute_call_info": tx_execution_info.execute_call_info,
        "fee_transfer_call_info": tx_execution_info.fee_transfer_call_info,
        "actual_fee": receipt.fee,
        "da_gas": receipt.da_gas,
        "actual_resources": resources_map,
        "revert_error": tx_execution_info.revert_error.as_ref().map(|err| err.to_string()),
        "total_gas": receipt.gas,
    });

    serde_json::to_string(&central_json).expect("Failed to serialize TransactionExecutionInfo")
}
