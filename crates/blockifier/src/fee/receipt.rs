use starknet_api::core::ContractAddress;
use starknet_api::executable_transaction::TransactionType;
use starknet_api::execution_resources::{GasAmount, GasVector};
use starknet_api::transaction::fields::{Fee, GasVectorComputationMode};

use crate::context::TransactionContext;
use crate::execution::call_info::ExecutionSummary;
use crate::fee::resources::{
    ComputationResources,
    StarknetResources,
    StateResources,
    TransactionResources,
};
use crate::state::cached_state::StateChanges;
use crate::transaction::account_transaction::AccountTransaction;
use crate::transaction::objects::HasRelatedFeeType;

#[cfg(test)]
#[path = "receipt_test.rs"]
pub mod test;

/// Parameters required to compute actual cost of a transaction.
struct TransactionReceiptParameters<'a> {
    tx_context: &'a TransactionContext,
    gas_mode: GasVectorComputationMode,
    calldata_length: usize,
    signature_length: usize,
    code_size: usize,
    state_changes: &'a StateChanges,
    sender_address: Option<ContractAddress>,
    l1_handler_payload_size: Option<usize>,
    execution_summary_without_fee_transfer: ExecutionSummary,
    tx_type: TransactionType,
    reverted_steps: usize,
    reverted_sierra_gas: GasAmount,
}

// TODO(Gilad): Use everywhere instead of passing the `actual_{fee,resources}` tuple, which often
// get passed around together.
#[cfg_attr(any(test, feature = "testing"), derive(Clone))]
#[cfg_attr(feature = "transaction_serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Default, Debug, PartialEq)]
pub struct TransactionReceipt {
    pub fee: Fee,
    pub gas: GasVector,
    pub da_gas: GasVector,
    pub resources: TransactionResources,
}

impl TransactionReceipt {
    fn from_params(tx_receipt_params: TransactionReceiptParameters<'_>) -> Self {
        let TransactionReceiptParameters {
            tx_context,
            gas_mode,
            calldata_length,
            signature_length,
            code_size,
            state_changes,
            sender_address,
            l1_handler_payload_size,
            execution_summary_without_fee_transfer,
            tx_type,
            reverted_steps,
            reverted_sierra_gas,
        } = tx_receipt_params;
        let charged_resources = execution_summary_without_fee_transfer.charged_resources.clone();
        let starknet_resources = StarknetResources::new(
            calldata_length,
            signature_length,
            code_size,
            StateResources::new(state_changes, sender_address, tx_context.fee_token_address()),
            l1_handler_payload_size,
            execution_summary_without_fee_transfer,
        );

        // Transaction overhead ('additional') resources are computed in VM resources no matter what
        // the tracked resources of the transaction are.
        let os_vm_resources = tx_context
            .block_context
            .versioned_constants
            .get_additional_os_tx_resources(
                tx_type,
                &starknet_resources,
                tx_context.block_context.block_info.use_kzg_da,
            )
            .filter_unused_builtins();

        let tx_resources = TransactionResources {
            starknet_resources,
            computation: ComputationResources {
                tx_vm_resources: charged_resources.vm_resources.filter_unused_builtins(),
                os_vm_resources,
                n_reverted_steps: reverted_steps,
                sierra_gas: charged_resources.gas_consumed,
                reverted_sierra_gas,
            },
        };

        let gas = tx_resources.to_gas_vector(
            &tx_context.block_context.versioned_constants,
            tx_context.block_context.block_info.use_kzg_da,
            &gas_mode,
        );
        // Backward-compatibility.
        let fee = if tx_type == TransactionType::Declare && tx_context.tx_info.is_v0() {
            Fee(0)
        } else {
            tx_context.tx_info.get_fee_by_gas_vector(
                &tx_context.block_context.block_info,
                gas,
                tx_context.effective_tip(),
            )
        };

        let da_gas = tx_resources
            .starknet_resources
            .state
            .da_gas_vector(tx_context.block_context.block_info.use_kzg_da);

        Self { resources: tx_resources, gas, da_gas, fee }
    }

    /// Computes the receipt of an L1 handler transaction.
    pub fn from_l1_handler<'a>(
        tx_context: &'a TransactionContext,
        l1_handler_payload_size: usize,
        execution_summary_without_fee_transfer: ExecutionSummary,
        state_changes: &'a StateChanges,
    ) -> Self {
        Self::from_params(TransactionReceiptParameters {
            tx_context,
            gas_mode: GasVectorComputationMode::All, /* Although L1 handler resources are
                                                      * deprecated, we still want to compute a
                                                      * full gas vector. */
            calldata_length: l1_handler_payload_size,
            signature_length: 0, // Signature is validated on L1.
            code_size: 0,
            state_changes,
            sender_address: None, // L1 handlers have no sender address.
            l1_handler_payload_size: Some(l1_handler_payload_size),
            execution_summary_without_fee_transfer,
            tx_type: TransactionType::L1Handler,
            reverted_steps: 0,
            reverted_sierra_gas: GasAmount(0),
        })
    }

    /// Computes the receipt of a reverted L1 handler transaction.
    pub fn reverted_l1_handler(
        tx_context: &TransactionContext,
        l1_handler_payload_size: usize,
    ) -> Self {
        Self::from_l1_handler(
            tx_context,
            l1_handler_payload_size,
            ExecutionSummary::default(),
            &StateChanges::default(),
        )
    }

    /// Computes the receipt of an account transaction.
    pub fn from_account_tx<'a>(
        account_tx: &'a AccountTransaction,
        tx_context: &'a TransactionContext,
        state_changes: &'a StateChanges,
        execution_summary_without_fee_transfer: ExecutionSummary,
        reverted_steps: usize,
        reverted_sierra_gas: GasAmount,
    ) -> Self {
        Self::from_params(TransactionReceiptParameters {
            tx_context,
            gas_mode: tx_context.get_gas_vector_computation_mode(),
            calldata_length: account_tx.calldata_length(),
            signature_length: account_tx.signature_length(),
            code_size: account_tx.declare_code_size(),
            state_changes,
            sender_address: Some(tx_context.tx_info.sender_address()),
            l1_handler_payload_size: None,
            execution_summary_without_fee_transfer,
            tx_type: account_tx.tx_type(),
            reverted_steps,
            reverted_sierra_gas,
        })
    }
}
