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

#[derive(Clone, Debug)]
pub enum CallInfoType {
    Validate,
    Execute,
    FeeTransfer,
}

pub fn call_info_order(tx_type: TransactionType) -> Vec<CallInfoType> {
    match tx_type {
        TransactionType::DeployAccount => {
            vec![CallInfoType::Execute, CallInfoType::Validate, CallInfoType::FeeTransfer]
        }
        _ => vec![CallInfoType::Validate, CallInfoType::Execute, CallInfoType::FeeTransfer],
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
    pub fn get_call_info(&self, call_info_type: &CallInfoType) -> Option<&CallInfo> {
        match call_info_type {
            CallInfoType::Validate => self.validate_call_info.as_ref(),
            CallInfoType::Execute => self.execute_call_info.as_ref(),
            CallInfoType::FeeTransfer => self.fee_transfer_call_info.as_ref(),
        }
    }

    pub fn call_info_iter(&self, tx_type: TransactionType) -> CallInfoIter<'_> {
        CallInfoIter::new(
            call_info_order(tx_type)
                .into_iter()
                .filter_map(|call_type| self.get_call_info(&call_type))
                .collect()
        )
    }
}


#[derive(Clone, Debug)]
struct LevelIndex {
    index: usize,
    length: usize,
}

impl LevelIndex {
    fn new(length: usize) -> Self {
        Self { index: 0, length }
    }
}

/// Keeps track of the current index in each level of a tree-like structure.
#[derive(Clone, Debug)]
struct TreeIndex {
    levels: Vec<LevelIndex>,
}

impl TreeIndex {
    fn new(first_level_length: usize) -> Self {
        Self { levels: vec![LevelIndex::new(first_level_length)] }
    }

    fn push_level(&mut self, length: usize) {
        self.levels.push(LevelIndex::new(length));
    }

    fn increment_index(&mut self) {
        // Iterate through levels in reverse order
        let mut i = self.levels.len();
        while i > 0 {
            i -= 1; // Decrement first to get the correct index
            self.levels[i].index += 1;
            if self.levels[i].index == self.levels[i].length {
                self.levels.remove(i);
            } else {
                break;
            }
        }
    }
}

/// Keeps track of the current call info index in a forest-like structure.
/// The index of the first tree level points to the corresponding call info type.
#[derive(Debug)]
pub struct CallInfoIndex {
    call_info_tree_index: TreeIndex,
    call_info_types: Vec<CallInfoType>,
}

impl CallInfoIndex {
    pub fn empty() -> Self {
        Self { call_info_tree_index: TreeIndex::new(0), call_info_types: Vec::new() }
    }

    pub fn new(
        tx_execution_info: &CentralTransactionExecutionInfo,
        tx_type: TransactionType,
    ) -> Self {
        let call_info_types: Vec<_> = call_info_order(tx_type)
            .into_iter()
            .filter(|call_info_type| tx_execution_info.get_call_info(call_info_type).is_some())
            .collect();
        Self { call_info_tree_index: TreeIndex::new(call_info_types.len()), call_info_types }
    }

    pub fn current_call_info<'a>(
        &self,
        tx_execution_info: &'a CentralTransactionExecutionInfo,
    ) -> Option<&'a CallInfo> {
        // Take call info type by the index of the first level.
        let call_info_types_index = self.call_info_tree_index.levels.get(0)?.index;
        let root_call_info =
            tx_execution_info.get_call_info(self.call_info_types.get(call_info_types_index)?)?;
        // Iterate through the levels to get the inner call info.
        let mut node = root_call_info;
        for i in 1..self.call_info_tree_index.levels.len() {
            node = node.inner_calls.get(self.call_info_tree_index.levels[i].index)?;
        }
        Some(node)
    }

    pub fn increment_call_info(&mut self, n_inner_calls: usize) {
        if n_inner_calls > 0 {
            self.call_info_tree_index.push_level(n_inner_calls);
        } else {
            self.call_info_tree_index.increment_index();
        }
    }
}
