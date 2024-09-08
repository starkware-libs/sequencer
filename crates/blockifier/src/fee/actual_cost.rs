use std::collections::HashMap;

use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use starknet_api::core::ContractAddress;
use starknet_api::transaction::Fee;

use crate::abi::constants as abi_constants;
use crate::context::TransactionContext;
use crate::execution::call_info::CallInfo;
use crate::state::cached_state::StateChanges;
use crate::transaction::account_transaction::AccountTransaction;
use crate::transaction::objects::{
    ExecutionResourcesTraits,
    GasVector,
    HasRelatedFeeType,
    ResourcesMapping,
    StarknetResources,
    TransactionExecutionResult,
    TransactionResources,
};
use crate::transaction::transaction_types::TransactionType;
use crate::utils::usize_from_u128;

#[cfg(test)]
#[path = "actual_cost_test.rs"]
pub mod test;

/// Parameters required to compute actual cost of a transaction.
struct TransactionReceiptParameters<'a, T: Iterator<Item = &'a CallInfo> + Clone> {
    tx_context: &'a TransactionContext,
    calldata_length: usize,
    signature_length: usize,
    code_size: usize,
    state_changes: &'a StateChanges,
    sender_address: Option<ContractAddress>,
    l1_handler_payload_size: Option<usize>,
    call_infos: T,
    execution_resources: &'a ExecutionResources,
    tx_type: TransactionType,
    reverted_steps: usize,
}

// TODO(Gilad): Use everywhere instead of passing the `actual_{fee,resources}` tuple, which often
// get passed around together.
#[cfg_attr(feature = "transaction_serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Default, Debug, PartialEq)]
pub struct TransactionReceipt {
    pub fee: Fee,
    pub gas: GasVector,
    pub da_gas: GasVector,
    pub resources: TransactionResources,
}

impl TransactionReceipt {
    fn from_params<'a, T: Iterator<Item = &'a CallInfo> + Clone>(
        tx_receipt_params: TransactionReceiptParameters<'a, T>,
    ) -> TransactionExecutionResult<Self> {
        let TransactionReceiptParameters {
            tx_context,
            calldata_length,
            signature_length,
            code_size,
            state_changes,
            sender_address,
            l1_handler_payload_size,
            call_infos,
            execution_resources,
            tx_type,
            reverted_steps,
        } = tx_receipt_params;

        let starknet_resources = StarknetResources::new(
            calldata_length,
            signature_length,
            code_size,
            state_changes.count_for_fee_charge(sender_address, tx_context.fee_token_address()),
            l1_handler_payload_size,
            call_infos,
        );

        let cairo_resources = (execution_resources
            + &tx_context.block_context.versioned_constants.get_additional_os_tx_resources(
                tx_type,
                &starknet_resources,
                tx_context.block_context.block_info.use_kzg_da,
            )?)
            .filter_unused_builtins();

        let tx_resources = TransactionResources {
            starknet_resources,
            vm_resources: cairo_resources,
            n_reverted_steps: reverted_steps,
        };

        let gas = tx_resources.to_gas_vector(
            &tx_context.block_context.versioned_constants,
            tx_context.block_context.block_info.use_kzg_da,
            &tx_context.get_gas_vector_computation_mode(),
        )?;

        // L1 handler transactions are not charged an L2 fee but it is compared to the L1 fee.
        let fee = if tx_context.tx_info.enforce_fee() || tx_type == TransactionType::L1Handler {
            tx_context.tx_info.get_fee_by_gas_vector(&tx_context.block_context.block_info, gas)
        } else {
            Fee(0)
        };
        let da_gas = tx_resources
            .starknet_resources
            .get_state_changes_cost(tx_context.block_context.block_info.use_kzg_da);

        Ok(Self { resources: tx_resources, gas, da_gas, fee })
    }

    pub fn to_resources_mapping(&self, with_reverted_steps: bool) -> ResourcesMapping {
        let GasVector { l1_gas, l1_data_gas, l2_gas } = self.gas;
        let mut resources = self.resources.vm_resources.to_resources_mapping();
        resources.0.extend(HashMap::from([
            (
                abi_constants::L1_GAS_USAGE.to_string(),
                usize_from_u128(l1_gas)
                    .expect("This conversion should not fail as the value is a converted usize."),
            ),
            (
                abi_constants::BLOB_GAS_USAGE.to_string(),
                usize_from_u128(l1_data_gas)
                    .expect("This conversion should not fail as the value is a converted usize."),
            ),
            (
                abi_constants::L2_GAS_USAGE.to_string(),
                usize_from_u128(l2_gas)
                    .expect("This conversion should not fail as the value is a converted usize."),
            ),
        ]));
        let reverted_steps_to_add =
            if with_reverted_steps { self.resources.n_reverted_steps } else { 0 };
        *resources.0.get_mut(abi_constants::N_STEPS_RESOURCE).unwrap_or(&mut 0) +=
            reverted_steps_to_add;
        resources
    }

    /// Computes actual cost of an L1 handler transaction.
    pub fn from_l1_handler<'a>(
        tx_context: &'a TransactionContext,
        l1_handler_payload_size: usize,
        call_infos: impl Iterator<Item = &'a CallInfo> + Clone,
        state_changes: &'a StateChanges,
        execution_resources: &'a ExecutionResources,
    ) -> TransactionExecutionResult<Self> {
        Self::from_params(TransactionReceiptParameters {
            tx_context,
            calldata_length: l1_handler_payload_size,
            signature_length: 0, // Signature is validated on L1.
            code_size: 0,
            state_changes,
            sender_address: None, // L1 handlers have no sender address.
            l1_handler_payload_size: Some(l1_handler_payload_size),
            call_infos,
            execution_resources,
            tx_type: TransactionType::L1Handler,
            reverted_steps: 0,
        })
    }

    /// Computes actual cost of an account transaction.
    pub fn from_account_tx<'a>(
        account_tx: &'a AccountTransaction,
        tx_context: &'a TransactionContext,
        state_changes: &'a StateChanges,
        execution_resources: &'a ExecutionResources,
        call_infos: impl Iterator<Item = &'a CallInfo> + Clone,
        reverted_steps: usize,
    ) -> TransactionExecutionResult<Self> {
        Self::from_params(TransactionReceiptParameters {
            tx_context,
            calldata_length: account_tx.calldata_length(),
            signature_length: account_tx.signature_length(),
            code_size: account_tx.declare_code_size(),
            state_changes,
            sender_address: Some(tx_context.tx_info.sender_address()),
            l1_handler_payload_size: None,
            call_infos,
            execution_resources,
            tx_type: account_tx.tx_type(),
            reverted_steps,
        })
    }
}
