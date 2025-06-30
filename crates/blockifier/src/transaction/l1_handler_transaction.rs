use std::sync::Arc;

use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::execution_resources::GasAmount;
use starknet_api::transaction::fields::{Fee, TransactionSignature};
use starknet_api::transaction::TransactionVersion;

use super::objects::RevertError;
use crate::context::BlockContext;
use crate::execution::call_info::CallInfo;
use crate::execution::entry_point::{EntryPointExecutionContext, SierraGasRevertTracker};
use crate::execution::stack_trace::gen_tx_execution_error_trace;
use crate::fee::fee_checks::FeeCheckReport;
use crate::fee::receipt::TransactionReceipt;
use crate::state::cached_state::TransactionalState;
use crate::state::state_api::UpdatableState;
use crate::transaction::errors::{TransactionExecutionError, TransactionFeeError};
use crate::transaction::objects::{
    CommonAccountFields,
    DeprecatedTransactionInfo,
    HasRelatedFeeType,
    TransactionExecutionInfo,
    TransactionExecutionResult,
    TransactionInfo,
    TransactionInfoCreator,
};
use crate::transaction::transactions::{Executable, ExecutableTransaction};

impl HasRelatedFeeType for L1HandlerTransaction {
    fn version(&self) -> TransactionVersion {
        self.tx.version
    }

    fn is_l1_handler(&self) -> bool {
        true
    }
}

impl TransactionInfoCreator for L1HandlerTransaction {
    fn create_tx_info(&self) -> TransactionInfo {
        TransactionInfo::Deprecated(DeprecatedTransactionInfo {
            common_fields: CommonAccountFields {
                transaction_hash: self.tx_hash,
                version: self.tx.version,
                signature: TransactionSignature::default(),
                nonce: self.tx.nonce,
                sender_address: self.tx.contract_address,
                only_query: false,
            },
            max_fee: Fee::default(),
        })
    }
}

impl<U: UpdatableState> ExecutableTransaction<U> for L1HandlerTransaction {
    fn execute_raw(
        &self,
        state: &mut TransactionalState<'_, U>,
        block_context: &BlockContext,
        _concurrency_mode: bool,
    ) -> TransactionExecutionResult<TransactionExecutionInfo> {
        let tx_context = Arc::new(block_context.to_tx_context(self));
        let limit_steps_by_resources = false;
        let l1_handler_bounds =
            block_context.versioned_constants.os_constants.l1_handler_max_amount_bounds;

        let mut remaining_gas = l1_handler_bounds.l2_gas.0;
        let mut context = EntryPointExecutionContext::new_invoke(
            tx_context.clone(),
            limit_steps_by_resources,
            SierraGasRevertTracker::new(GasAmount(remaining_gas)),
        );
        let l1_handler_payload_size = self.payload_size();

        let execution_result = self.run_execute(state, &mut context, &mut remaining_gas);
        match execution_result {
            Ok(execute_call_info) => {
                let receipt = TransactionReceipt::from_l1_handler(
                    &tx_context,
                    l1_handler_payload_size,
                    CallInfo::summarize_many(
                        execute_call_info.iter(),
                        &block_context.versioned_constants,
                    ),
                    &state.to_state_diff()?,
                );

                // Enforce resource bounds.
                let fee_check_report = FeeCheckReport::check_all_gas_amounts_within_bounds(
                    &l1_handler_bounds,
                    &receipt.gas,
                );
                match fee_check_report {
                    Ok(()) => {
                        // TODO(Arni): Consider removing this check. It is covered by the starknet
                        // core contract.
                        let paid_fee = self.paid_fee_on_l1;
                        // For now, assert only that any amount of fee was paid.
                        // The error message still indicates the required fee.
                        if paid_fee == Fee(0) {
                            return Err(TransactionExecutionError::TransactionFeeError(Box::new(
                                TransactionFeeError::InsufficientFee {
                                    paid_fee,
                                    actual_fee: receipt.fee,
                                },
                            )));
                        }

                        Ok(l1_handler_tx_execution_info(execute_call_info, receipt, None))
                    }
                    Err(fee_check_error) => {
                        let receipt = TransactionReceipt::reverted_l1_handler(
                            &tx_context,
                            l1_handler_payload_size,
                        );
                        Ok(l1_handler_tx_execution_info(
                            None,
                            receipt,
                            Some(fee_check_error.into()),
                        ))
                    }
                }
            }
            Err(execution_error) => {
                let receipt =
                    TransactionReceipt::reverted_l1_handler(&tx_context, l1_handler_payload_size);
                Ok(l1_handler_tx_execution_info(
                    None,
                    receipt,
                    Some(gen_tx_execution_error_trace(&execution_error).into()),
                ))
            }
        }
    }
}

fn l1_handler_tx_execution_info(
    execute_call_info: Option<CallInfo>,
    mut receipt: TransactionReceipt,
    revert_error: Option<RevertError>,
) -> TransactionExecutionInfo {
    receipt.fee = Fee(0);
    TransactionExecutionInfo {
        validate_call_info: None,
        execute_call_info,
        fee_transfer_call_info: None,
        receipt,
        revert_error,
    }
}
