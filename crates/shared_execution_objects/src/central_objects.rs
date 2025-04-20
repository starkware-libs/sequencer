use std::collections::HashMap;

use blockifier::abi::constants as abi_constants;
use blockifier::bouncer::CasmHashComputationData;
use blockifier::execution::call_info::CallInfo;
use blockifier::fee::receipt::TransactionReceipt;
use blockifier::transaction::objects::{ExecutionResourcesTraits, TransactionExecutionInfo};
use serde::Serialize;
use starknet_api::execution_resources::GasVector;
use starknet_api::transaction::fields::Fee;

/// A mapping from a transaction execution resource to its actual usage.
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct ResourcesMapping(pub HashMap<String, usize>);

impl From<TransactionReceipt> for ResourcesMapping {
    fn from(receipt: TransactionReceipt) -> ResourcesMapping {
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

#[derive(Serialize, Debug)]
pub struct CentralCasmHashComputationData {
    pub class_hash_to_casm_hash_computation_gas: HashMap<String, u64>,
    pub sierra_gas_without_casm_hash_computation: u64,
}

impl From<CasmHashComputationData> for CentralCasmHashComputationData {
    fn from(data: CasmHashComputationData) -> Self {
        Self {
            class_hash_to_casm_hash_computation_gas: data
                .class_hash_to_casm_hash_computation_gas
                .iter()
                .map(|(class_hash, gas)| (class_hash.0.to_hex_string(), gas.0))
                .collect(),
            sierra_gas_without_casm_hash_computation: data
                .sierra_gas_without_casm_hash_computation
                .0,
        }
    }
}
