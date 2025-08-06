use std::sync::Arc;
use std::time::{Duration, Instant};

use log::warn;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::executable_transaction::AccountTransaction as ApiTransaction;
use starknet_api::execution_resources::GasAmount;
use thiserror::Error;

use crate::blockifier::config::TransactionExecutorConfig;
use crate::blockifier::transaction_executor::{
    TransactionExecutor,
    TransactionExecutorError,
    BLOCK_STATE_ACCESS_ERR,
};
use crate::context::{BlockContext, GasCounter, TransactionContext};
use crate::execution::call_info::CallInfo;
use crate::fee::fee_checks::PostValidationReport;
use crate::fee::receipt::TransactionReceipt;
use crate::state::cached_state::CachedState;
use crate::state::errors::StateError;
use crate::state::state_api::StateReader;
use crate::transaction::account_transaction::AccountTransaction;
use crate::transaction::errors::{TransactionExecutionError, TransactionPreValidationError};
use crate::transaction::transaction_execution::Transaction;
use crate::transaction::transactions::ValidatableTransaction;

#[cfg(test)]
#[path = "stateful_validator_test.rs"]
pub mod stateful_validator_test;

#[derive(Debug, Error)]
pub enum StatefulValidatorError {
    #[error(transparent)]
    StateError(#[from] StateError),
    #[error(transparent)]
    TransactionExecutionError(#[from] TransactionExecutionError),
    #[error(transparent)]
    TransactionExecutorError(#[from] TransactionExecutorError),
    #[error(transparent)]
    TransactionPreValidationError(#[from] TransactionPreValidationError),
}

pub type StatefulValidatorResult<T> = Result<T, StatefulValidatorError>;

/// Manages state related transaction validations for pre-execution flows.
pub struct StatefulValidator<S: StateReader> {
    tx_executor: TransactionExecutor<S>,
}

impl<S: StateReader> StatefulValidator<S> {
    pub fn create(state: CachedState<S>, block_context: BlockContext) -> Self {
        let tx_executor =
            TransactionExecutor::new(state, block_context, TransactionExecutorConfig::default());
        Self { tx_executor }
    }

    #[allow(clippy::result_large_err)]
    pub fn perform_validations(&mut self, tx: AccountTransaction) -> StatefulValidatorResult<()> {
        let mut checkpoints = Vec::new();
        checkpoints.push(("start", Instant::now()));

        // Deploy account transaction should be fully executed, since the constructor must run
        // before `__validate_deploy__`. The execution already includes all necessary validations,
        // so they are skipped here.
        // Declare transaction should also be fully executed - otherwise, if we only go through
        // the validate phase, we would miss the check that the class was not declared before.
        if let ApiTransaction::DeployAccount(_) | ApiTransaction::Declare(_) = tx.tx {
            return self.execute(tx);
        }

        let tx_context = Arc::new(self.tx_executor.block_context.to_tx_context(&tx));
        tx.perform_pre_validation_stage(self.state(), &tx_context)?;

        checkpoints.push(("pre_validate", Instant::now()));
        if !tx.execution_flags.validate {
            return Ok(());
        }

        // `__validate__` call.
        let (_optional_call_info, actual_cost) = self.validate(&tx, tx_context.clone())?;

        checkpoints.push(("tx_validate", Instant::now()));

        // Post validations.
        PostValidationReport::verify(&tx_context, &actual_cost, tx.execution_flags.charge_fee)?;

        checkpoints.push(("post_validate", Instant::now()));

        let total_duration =
            checkpoints.last().unwrap().1.duration_since(checkpoints.first().unwrap().1);
        if total_duration > Duration::from_secs(1) {
            // print the checkpoint diffs from prev checkpoint in a single warn!
            let mut diffs = String::new();
            for pair in checkpoints.windows(2) {
                diffs.push_str(&format!(
                    "{}: {:?}, ",
                    pair[0].0,
                    pair[1].1.duration_since(pair[0].1)
                ));
            }
            warn!("High latency in stateful validator: {:?}. total: {:?}", diffs, total_duration);
        }

        Ok(())
    }

    pub fn block_context(&self) -> &BlockContext {
        self.tx_executor.block_context.as_ref()
    }

    fn state(&mut self) -> &mut CachedState<S> {
        self.tx_executor.block_state.as_mut().expect(BLOCK_STATE_ACCESS_ERR)
    }

    #[allow(clippy::result_large_err)]
    fn execute(&mut self, tx: AccountTransaction) -> StatefulValidatorResult<()> {
        self.tx_executor.execute(&Transaction::Account(tx))?;
        Ok(())
    }

    #[allow(clippy::result_large_err)]
    fn validate(
        &mut self,
        tx: &AccountTransaction,
        tx_context: Arc<TransactionContext>,
    ) -> StatefulValidatorResult<(Option<CallInfo>, TransactionReceipt)> {
        let validate_call_info = tx.validate_tx(
            self.state(),
            tx_context.clone(),
            &mut GasCounter::new(tx_context.initial_sierra_gas()),
        )?;

        let tx_receipt = TransactionReceipt::from_account_tx(
            tx,
            &tx_context,
            &self.state().to_state_diff()?,
            CallInfo::summarize_many(
                validate_call_info.iter(),
                &tx_context.block_context.versioned_constants,
            ),
            0,
            GasAmount(0),
        );

        Ok((validate_call_info, tx_receipt))
    }

    #[allow(clippy::result_large_err)]
    pub fn get_nonce(
        &mut self,
        account_address: ContractAddress,
    ) -> StatefulValidatorResult<Nonce> {
        Ok(self.state().get_nonce_at(account_address)?)
    }
}
