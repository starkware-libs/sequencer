use std::sync::Arc;

use starknet_api::contract_class::EntryPointType;
use starknet_api::core::ContractAddress;
use starknet_api::executable_transaction::L1HandlerTransaction;
use starknet_api::execution_resources::GasAmount;
use starknet_api::transaction::fields::{Calldata, Fee, TransactionSignature};
use starknet_api::transaction::TransactionVersion;

use crate::context::BlockContext;
use crate::execution::call_info::CallInfo;
use crate::execution::entry_point::{
    CallEntryPoint,
    CallType,
    EntryPointExecutionContext,
    SierraGasRevertTracker,
};
use crate::fee::fee_checks::FeeCheckReport;
use crate::fee::receipt::TransactionReceipt;
use crate::state::cached_state::TransactionalState;
use crate::state::state_api::{State, UpdatableState};
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
        let execute_call_info = self.run_execute(state, &mut context, &mut remaining_gas)?;
        let l1_handler_payload_size = self.payload_size();
        let mut receipt = TransactionReceipt::from_l1_handler(
            &tx_context,
            l1_handler_payload_size,
            CallInfo::summarize_many(execute_call_info.iter(), &block_context.versioned_constants),
            &state.to_state_diff()?,
        );

        // Enforce resource bounds.
        FeeCheckReport::check_all_gas_amounts_within_bounds(&l1_handler_bounds, &receipt.gas)?;

        let paid_fee = self.paid_fee_on_l1;
        // For now, assert only that any amount of fee was paid.
        // The error message still indicates the required fee.
        if paid_fee == Fee(0) {
            return Err(TransactionExecutionError::TransactionFeeError(
                TransactionFeeError::InsufficientFee { paid_fee, actual_fee: receipt.fee },
            ));
        }

        receipt.fee = Fee(0);
        Ok(TransactionExecutionInfo {
            validate_call_info: None,
            execute_call_info,
            fee_transfer_call_info: None,
            receipt,
            revert_error: None,
        })
    }
}

impl<S: State> Executable<S> for L1HandlerTransaction {
    fn run_execute(
        &self,
        state: &mut S,
        context: &mut EntryPointExecutionContext,
        remaining_gas: &mut u64,
    ) -> TransactionExecutionResult<Option<CallInfo>> {
        let tx = &self.tx;
        let storage_address = tx.contract_address;
        let class_hash = state.get_class_hash_at(storage_address)?;
        let selector = tx.entry_point_selector;
        let execute_call = CallEntryPoint {
            entry_point_type: EntryPointType::L1Handler,
            entry_point_selector: selector,
            calldata: Calldata(Arc::clone(&tx.calldata.0)),
            class_hash: None,
            code_address: None,
            storage_address,
            caller_address: ContractAddress::default(),
            call_type: CallType::Call,
            initial_gas: *remaining_gas,
        };

        execute_call.non_reverting_execute(state, context, remaining_gas).map(Some).map_err(
            |error| TransactionExecutionError::ExecutionError {
                error,
                class_hash,
                storage_address,
                selector,
            },
        )
    }
}
