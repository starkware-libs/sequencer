use std::collections::HashMap;

use blockifier::abi::constants as abi_constants;
use blockifier::execution::call_info::{CairoPrimitiveName, CallInfo, CallInfoIter};
use blockifier::transaction::objects::{ExecutionResourcesTraits, TransactionExecutionInfo};
use serde::Serialize;
use starknet_api::execution_resources::GasVector;
use starknet_api::transaction::constants::VALIDATE_DEPLOY_ENTRY_POINT_SELECTOR;
use starknet_api::transaction::fields::Fee;

/// A mapping from a transaction execution resource to its actual usage.
#[cfg_attr(feature = "deserialize", derive(serde::Deserialize))]
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct ResourcesMapping(pub HashMap<String, usize>);

fn build_actual_resources(tx_execution_info: &TransactionExecutionInfo) -> ResourcesMapping {
    let computation = &tx_execution_info.receipt.resources.computation;
    let vm_resources = computation.total_extended_vm_resources().vm_resources;
    let mut resources = HashMap::from([(
        abi_constants::N_STEPS_RESOURCE.to_string(),
        vm_resources.total_n_steps() + computation.n_reverted_steps,
    )]);
    resources.extend(
        vm_resources
            .prover_builtins()
            .into_iter()
            .map(|(builtin, count)| (builtin.to_str_with_suffix().to_string(), count)),
    );

    // Opcode counters live in `CallInfo.builtin_counters` for SierraGas-tracked
    // calls (Cairo 1 in CASM and Native), where `resources.opcode_instance_counter`
    // is intentionally zeroed to avoid double-charging Sierra gas. Pull them in
    // from `summarize_builtins()` — the same source the bouncer's proving-gas
    // path uses — so the count is reported even when the receipt's
    // `opcode_instance_counter` is empty.
    for (primitive, count) in tx_execution_info.summarize_builtins() {
        if let CairoPrimitiveName::Opcode(opcode) = primitive {
            *resources.entry(opcode.to_str_with_suffix().to_string()).or_insert(0) += count;
        }
    }

    ResourcesMapping(resources)
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
        let actual_resources = build_actual_resources(&tx_execution_info);
        CentralTransactionExecutionInfo {
            validate_call_info: tx_execution_info.validate_call_info,
            execute_call_info: tx_execution_info.execute_call_info,
            fee_transfer_call_info: tx_execution_info.fee_transfer_call_info,
            actual_fee: tx_execution_info.receipt.fee,
            da_gas: tx_execution_info.receipt.da_gas,
            revert_error: tx_execution_info.revert_error.map(|error| error.to_string()),
            total_gas: tx_execution_info.receipt.gas,
            actual_resources,
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
