use std::sync::Arc;

use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::contract_class::EntryPointType;
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress};
use starknet_api::executable_transaction::{
    AccountTransaction,
    DeclareTransaction,
    DeployAccountTransaction,
    InvokeTransaction,
    L1HandlerTransaction,
};
use starknet_api::transaction::fields::{AccountDeploymentData, Calldata};
use starknet_api::transaction::{constants, DeclareTransactionV2, DeclareTransactionV3};

use crate::context::{BlockContext, GasCounter, TransactionContext};
use crate::execution::call_info::CallInfo;
use crate::execution::entry_point::{
    CallEntryPoint,
    CallType,
    ConstructorContext,
    EntryPointExecutionContext,
};
use crate::execution::execution_utils::execute_deployment;
use crate::state::cached_state::TransactionalState;
use crate::state::errors::StateError;
use crate::state::state_api::{State, UpdatableState};
use crate::transaction::errors::TransactionExecutionError;
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
                error: Box::new(error),
                class_hash,
                storage_address,
                selector,
            },
        )
    }
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

impl<S: State> Executable<S> for DeclareTransaction {
    fn run_execute(
        &self,
        state: &mut S,
        context: &mut EntryPointExecutionContext,
        _remaining_gas: &mut u64,
    ) -> TransactionExecutionResult<Option<CallInfo>> {
        let class_hash = self.class_hash();
        match &self.tx {
            starknet_api::transaction::DeclareTransaction::V0(_)
            | starknet_api::transaction::DeclareTransaction::V1(_) => {
                if context.tx_context.block_context.versioned_constants.disable_cairo0_redeclaration
                {
                    try_declare(self, state, class_hash, None)?
                } else {
                    // We allow redeclaration of the class for backward compatibility.
                    // In the past, we allowed redeclaration of Cairo 0 contracts since there was
                    // no class commitment (so no need to check if the class is already declared).
                    state.set_contract_class(class_hash, self.contract_class().try_into()?)?;
                }
            }
            starknet_api::transaction::DeclareTransaction::V2(DeclareTransactionV2 {
                compiled_class_hash,
                ..
            })
            | starknet_api::transaction::DeclareTransaction::V3(DeclareTransactionV3 {
                compiled_class_hash,
                ..
            }) => try_declare(self, state, class_hash, Some(*compiled_class_hash))?,
        }
        Ok(None)
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

impl<S: State> Executable<S> for DeployAccountTransaction {
    fn run_execute(
        &self,
        state: &mut S,
        context: &mut EntryPointExecutionContext,
        remaining_gas: &mut u64,
    ) -> TransactionExecutionResult<Option<CallInfo>> {
        let class_hash = self.class_hash();
        let constructor_context = ConstructorContext {
            class_hash,
            code_address: None,
            storage_address: self.contract_address(),
            caller_address: ContractAddress::default(),
        };
        let call_info = execute_deployment(
            state,
            context,
            constructor_context,
            self.constructor_calldata(),
            remaining_gas,
        )?;

        Ok(Some(call_info))
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

impl<S: State> Executable<S> for InvokeTransaction {
    fn run_execute(
        &self,
        state: &mut S,
        context: &mut EntryPointExecutionContext,
        remaining_gas: &mut u64,
    ) -> TransactionExecutionResult<Option<CallInfo>> {
        let entry_point_selector = match &self.tx {
            starknet_api::transaction::InvokeTransaction::V0(tx) => tx.entry_point_selector,
            starknet_api::transaction::InvokeTransaction::V1(_)
            | starknet_api::transaction::InvokeTransaction::V3(_) => {
                selector_from_name(constants::EXECUTE_ENTRY_POINT_NAME)
            }
        };
        let storage_address = context.tx_context.tx_info.sender_address();
        let class_hash = state.get_class_hash_at(storage_address)?;
        let execute_call = CallEntryPoint {
            entry_point_type: EntryPointType::External,
            entry_point_selector,
            calldata: self.calldata(),
            class_hash: None,
            code_address: None,
            storage_address,
            caller_address: ContractAddress::default(),
            call_type: CallType::Call,
            initial_gas: *remaining_gas,
        };

        let call_info =
            execute_call.non_reverting_execute(state, context, remaining_gas).map_err(|error| {
                TransactionExecutionError::ExecutionError {
                    error: Box::new(error),
                    class_hash,
                    storage_address,
                    selector: entry_point_selector,
                }
            })?;
        Ok(Some(call_info))
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
    // TODO(AvivG): Consider implemetation without 'create_tx_info'.
    tx.create_tx_info(only_query).enforce_fee()
}

/// Attempts to declare a contract class by setting the contract class in the state with the
/// specified class hash.
fn try_declare<S: State>(
    tx: &DeclareTransaction,
    state: &mut S,
    class_hash: ClassHash,
    compiled_class_hash: Option<CompiledClassHash>,
) -> TransactionExecutionResult<()> {
    match state.get_compiled_class(class_hash) {
        Err(StateError::UndeclaredClassHash(_)) => {
            // Class is undeclared; declare it.
            state.set_contract_class(class_hash, tx.contract_class().try_into()?)?;
            if let Some(compiled_class_hash) = compiled_class_hash {
                state.set_compiled_class_hash(class_hash, compiled_class_hash)?;
            }
            Ok(())
        }
        Err(error) => Err(error)?,
        Ok(_) => {
            // Class is already declared, cannot redeclare.
            Err(TransactionExecutionError::DeclareTransactionError { class_hash })
        }
    }
}
