use std::sync::Arc;

use starknet_api::executable_transaction::{
    AccountTransaction,
    DeclareTransaction,
    DeployAccountTransaction,
    InvokeTransaction,
};
use starknet_api::transaction::fields::AccountDeploymentData;

use crate::context::{BlockContext, GasCounter, TransactionContext};
use crate::execution::call_info::CallInfo;
use crate::execution::entry_point::EntryPointExecutionContext;
use crate::state::cached_state::TransactionalState;
use crate::state::state_api::{State, UpdatableState};
use crate::transaction::objects::{
    CommonAccountFields,
    CurrentTransactionInfo,
    DeprecatedTransactionInfo,
    TransactionExecutionInfo,
    TransactionExecutionResult,
    TransactionInfo,
    TransactionInfoCreatorInner,
};
#[cfg(test)]
#[path = "transactions_test.rs"]
mod test;

#[derive(Clone, Copy, Debug)]
pub struct ExecutionFlags {
    pub concurrency_mode: bool,
}

pub trait ExecutableTransaction<U: UpdatableState>: Sized {
    /// Executes the transaction in a transactional manner
    /// (if it fails, given state does not modify).
    fn execute(
        &self,
        state: &mut U,
        block_context: &BlockContext,
    ) -> TransactionExecutionResult<TransactionExecutionInfo> {
        log::debug!("Executing Transaction...");
        let mut transactional_state = TransactionalState::create_transactional(state);
        let concurrency_mode = false;
        let execution_result =
            self.execute_raw(&mut transactional_state, block_context, concurrency_mode);

        match execution_result {
            Ok(value) => {
                transactional_state.commit();
                log::debug!("Transaction execution complete and committed.");
                Ok(value)
            }
            Err(error) => {
                log::debug!("Transaction execution failed with: {error}");
                transactional_state.abort();
                Err(error)
            }
        }
    }

    /// Note: In case of execution failure, the state may become corrupted. This means that
    /// any changes made up to the point of failure will persist in the state. To revert these
    /// changes, you should call `state.abort()`. Alternatively, consider using `execute`
    /// for automatic handling of such cases.
    fn execute_raw(
        &self,
        state: &mut TransactionalState<'_, U>,
        block_context: &BlockContext,
        concurrency_mode: bool,
    ) -> TransactionExecutionResult<TransactionExecutionInfo>;
}

pub trait Executable<S: State> {
    fn run_execute(
        &self,
        state: &mut S,
        context: &mut EntryPointExecutionContext,
        // TODO(Arni): Use the struct GasCounter?
        remaining_gas: &mut u64,
    ) -> TransactionExecutionResult<Option<CallInfo>>;
}

/// Intended for use in sequencer pre-execution flows, like in a gateway service.
pub trait ValidatableTransaction {
    fn validate_tx(
        &self,
        state: &mut dyn State,
        tx_context: Arc<TransactionContext>,
        remaining_gas: &mut GasCounter,
    ) -> TransactionExecutionResult<Option<CallInfo>>;
}

impl TransactionInfoCreatorInner for AccountTransaction {
    fn create_tx_info(&self, only_query: bool) -> TransactionInfo {
        match self {
            Self::Declare(tx) => tx.create_tx_info(only_query),
            Self::DeployAccount(tx) => tx.create_tx_info(only_query),
            Self::Invoke(tx) => tx.create_tx_info(only_query),
        }
    }
}

impl TransactionInfoCreatorInner for DeclareTransaction {
    fn create_tx_info(&self, only_query: bool) -> TransactionInfo {
        // TODO(Nir, 01/11/2023): Consider to move this (from all get_tx_info methods).
        let common_fields = CommonAccountFields {
            transaction_hash: self.tx_hash,
            version: self.version(),
            signature: self.signature(),
            nonce: self.nonce(),
            sender_address: self.sender_address(),
            only_query,
        };

        match &self.tx {
            starknet_api::transaction::DeclareTransaction::V0(tx)
            | starknet_api::transaction::DeclareTransaction::V1(tx) => {
                TransactionInfo::Deprecated(DeprecatedTransactionInfo {
                    common_fields,
                    max_fee: tx.max_fee,
                })
            }
            starknet_api::transaction::DeclareTransaction::V2(tx) => {
                TransactionInfo::Deprecated(DeprecatedTransactionInfo {
                    common_fields,
                    max_fee: tx.max_fee,
                })
            }
            starknet_api::transaction::DeclareTransaction::V3(tx) => {
                TransactionInfo::Current(CurrentTransactionInfo {
                    common_fields,
                    resource_bounds: tx.resource_bounds,
                    tip: tx.tip,
                    nonce_data_availability_mode: tx.nonce_data_availability_mode,
                    fee_data_availability_mode: tx.fee_data_availability_mode,
                    paymaster_data: tx.paymaster_data.clone(),
                    account_deployment_data: tx.account_deployment_data.clone(),
                })
            }
        }
    }
}

impl TransactionInfoCreatorInner for DeployAccountTransaction {
    fn create_tx_info(&self, only_query: bool) -> TransactionInfo {
        let common_fields = CommonAccountFields {
            transaction_hash: self.tx_hash(),
            version: self.version(),
            signature: self.signature(),
            nonce: self.nonce(),
            sender_address: self.contract_address(),
            only_query,
        };

        match &self.tx {
            starknet_api::transaction::DeployAccountTransaction::V1(tx) => {
                TransactionInfo::Deprecated(DeprecatedTransactionInfo {
                    common_fields,
                    max_fee: tx.max_fee,
                })
            }
            starknet_api::transaction::DeployAccountTransaction::V3(tx) => {
                TransactionInfo::Current(CurrentTransactionInfo {
                    common_fields,
                    resource_bounds: tx.resource_bounds,
                    tip: tx.tip,
                    nonce_data_availability_mode: tx.nonce_data_availability_mode,
                    fee_data_availability_mode: tx.fee_data_availability_mode,
                    paymaster_data: tx.paymaster_data.clone(),
                    account_deployment_data: AccountDeploymentData::default(),
                })
            }
        }
    }
}

impl TransactionInfoCreatorInner for InvokeTransaction {
    fn create_tx_info(&self, only_query: bool) -> TransactionInfo {
        let common_fields = CommonAccountFields {
            transaction_hash: self.tx_hash(),
            version: self.version(),
            signature: self.signature(),
            nonce: self.nonce(),
            sender_address: self.sender_address(),
            only_query,
        };

        match &self.tx() {
            starknet_api::transaction::InvokeTransaction::V0(tx) => {
                TransactionInfo::Deprecated(DeprecatedTransactionInfo {
                    common_fields,
                    max_fee: tx.max_fee,
                })
            }
            starknet_api::transaction::InvokeTransaction::V1(tx) => {
                TransactionInfo::Deprecated(DeprecatedTransactionInfo {
                    common_fields,
                    max_fee: tx.max_fee,
                })
            }
            starknet_api::transaction::InvokeTransaction::V3(tx) => {
                TransactionInfo::Current(CurrentTransactionInfo {
                    common_fields,
                    resource_bounds: tx.resource_bounds,
                    tip: tx.tip,
                    nonce_data_availability_mode: tx.nonce_data_availability_mode,
                    fee_data_availability_mode: tx.fee_data_availability_mode,
                    paymaster_data: tx.paymaster_data.clone(),
                    account_deployment_data: tx.account_deployment_data.clone(),
                })
            }
        }
    }
}

/// Determines whether the fee should be enforced for the given transaction.
pub fn enforce_fee(tx: &AccountTransaction, only_query: bool) -> bool {
    // TODO(AvivG): Consider implementation without 'create_tx_info'.
    tx.create_tx_info(only_query).enforce_fee()
}
