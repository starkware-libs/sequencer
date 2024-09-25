use std::sync::Arc;

use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use starknet_api::contract_class::{ContractClass, EntryPointType};
use starknet_api::core::{ClassHash, CompiledClassHash, ContractAddress, Nonce};
use starknet_api::executable_transaction::{
    DeclareTransaction as ExecutableDeclareTx,
    DeployAccountTransaction as ExecutableDeployAccountTx,
    InvokeTransaction as ExecutableInvokeTx,
    L1HandlerTransaction,
};
use starknet_api::transaction::{
    AccountDeploymentData,
    Calldata,
    ContractAddressSalt,
    DeclareTransactionV2,
    DeclareTransactionV3,
    Fee,
    TransactionHash,
    TransactionSignature,
    TransactionVersion,
};

use crate::abi::abi_utils::selector_from_name;
use crate::context::{BlockContext, TransactionContext};
use crate::execution::call_info::CallInfo;
use crate::execution::contract_class::ClassInfo;
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
use crate::transaction::account_transaction::is_cairo1;
use crate::transaction::constants;
use crate::transaction::errors::TransactionExecutionError;
use crate::transaction::objects::{
    CommonAccountFields,
    CurrentTransactionInfo,
    DeprecatedTransactionInfo,
    HasRelatedFeeType,
    TransactionExecutionInfo,
    TransactionExecutionResult,
    TransactionInfo,
    TransactionInfoCreator,
};
#[cfg(test)]
#[path = "transactions_test.rs"]
mod test;

macro_rules! implement_inner_tx_getter_calls {
    ($(($field:ident, $field_type:ty)),*) => {
        $(pub fn $field(&self) -> $field_type {
            self.tx.$field().clone()
        })*
    };
}

#[derive(Clone, Copy, Debug)]
pub struct ExecutionFlags {
    pub charge_fee: bool,
    pub validate: bool,
    pub concurrency_mode: bool,
}

pub trait ExecutableTransaction<U: UpdatableState>: Sized {
    /// Executes the transaction in a transactional manner
    /// (if it fails, given state does not modify).
    fn execute(
        &self,
        state: &mut U,
        block_context: &BlockContext,
        charge_fee: bool,
        validate: bool,
    ) -> TransactionExecutionResult<TransactionExecutionInfo> {
        log::debug!("Executing Transaction...");
        let mut transactional_state = TransactionalState::create_transactional(state);
        let execution_flags = ExecutionFlags { charge_fee, validate, concurrency_mode: false };
        let execution_result =
            self.execute_raw(&mut transactional_state, block_context, execution_flags);

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
        execution_flags: ExecutionFlags,
    ) -> TransactionExecutionResult<TransactionExecutionInfo>;
}

pub trait Executable<S: State> {
    fn run_execute(
        &self,
        state: &mut S,
        resources: &mut ExecutionResources,
        context: &mut EntryPointExecutionContext,
        remaining_gas: &mut u64,
    ) -> TransactionExecutionResult<Option<CallInfo>>;
}

/// Intended for use in sequencer pre-execution flows, like in a gateway service.
pub trait ValidatableTransaction {
    fn validate_tx(
        &self,
        state: &mut dyn State,
        resources: &mut ExecutionResources,
        tx_context: Arc<TransactionContext>,
        remaining_gas: &mut u64,
        limit_steps_by_resources: bool,
    ) -> TransactionExecutionResult<Option<CallInfo>>;
}

#[derive(Clone, Debug)]
pub struct DeclareTransaction {
    pub tx: starknet_api::transaction::DeclareTransaction,
    pub tx_hash: TransactionHash,
    // Indicates the presence of the only_query bit in the version.
    only_query: bool,
    pub class_info: ClassInfo,
}

impl TryFrom<starknet_api::executable_transaction::DeclareTransaction> for DeclareTransaction {
    type Error = TransactionExecutionError;

    fn try_from(
        declare_tx: starknet_api::executable_transaction::DeclareTransaction,
    ) -> Result<Self, Self::Error> {
        Self::new_from_executable_tx(declare_tx, false)
    }
}

impl DeclareTransaction {
    fn create(
        declare_tx: starknet_api::transaction::DeclareTransaction,
        tx_hash: TransactionHash,
        class_info: ClassInfo,
        only_query: bool,
    ) -> TransactionExecutionResult<Self> {
        let declare_version = declare_tx.version();
        // Verify contract class version.
        // TODO(Noa): Avoid the unnecessary conversion.
        if !is_cairo1(&class_info.contract_class().try_into()?) {
            if declare_version > TransactionVersion::ONE {
                Err(TransactionExecutionError::ContractClassVersionMismatch {
                    declare_version,
                    cairo_version: 0,
                })?
            }
        } else if declare_version <= TransactionVersion::ONE {
            Err(TransactionExecutionError::ContractClassVersionMismatch {
                declare_version,
                cairo_version: 1,
            })?
        }
        Ok(Self { tx: declare_tx, tx_hash, class_info, only_query })
    }

    pub fn new(
        declare_tx: starknet_api::transaction::DeclareTransaction,
        tx_hash: TransactionHash,
        class_info: ClassInfo,
    ) -> TransactionExecutionResult<Self> {
        Self::create(declare_tx, tx_hash, class_info, false)
    }

    pub fn new_for_query(
        declare_tx: starknet_api::transaction::DeclareTransaction,
        tx_hash: TransactionHash,
        class_info: ClassInfo,
    ) -> TransactionExecutionResult<Self> {
        Self::create(declare_tx, tx_hash, class_info, true)
    }

    fn new_from_executable_tx(
        declare_tx: starknet_api::executable_transaction::DeclareTransaction,
        only_query: bool,
    ) -> Result<Self, TransactionExecutionError> {
        let starknet_api::executable_transaction::DeclareTransaction { tx, tx_hash, class_info } =
            declare_tx;
        let class_info = class_info.try_into()?;

        Self::create(tx, tx_hash, class_info, only_query)
    }

    implement_inner_tx_getter_calls!(
        (class_hash, ClassHash),
        (nonce, Nonce),
        (sender_address, ContractAddress),
        (signature, TransactionSignature),
        (version, TransactionVersion)
    );

    pub fn tx(&self) -> &starknet_api::transaction::DeclareTransaction {
        &self.tx
    }

    pub fn tx_hash(&self) -> TransactionHash {
        self.tx_hash
    }

    pub fn contract_class(&self) -> ContractClass {
        self.class_info.contract_class()
    }

    pub fn only_query(&self) -> bool {
        self.only_query
    }

    fn try_declare<S: State>(
        &self,
        state: &mut S,
        class_hash: ClassHash,
        compiled_class_hash: Option<CompiledClassHash>,
    ) -> TransactionExecutionResult<()> {
        match state.get_compiled_contract_class(class_hash) {
            Err(StateError::UndeclaredClassHash(_)) => {
                // Class is undeclared; declare it.
                state.set_contract_class(class_hash, self.contract_class().try_into()?)?;
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
}

impl<S: State> Executable<S> for DeclareTransaction {
    fn run_execute(
        &self,
        state: &mut S,
        _resources: &mut ExecutionResources,
        context: &mut EntryPointExecutionContext,
        _remaining_gas: &mut u64,
    ) -> TransactionExecutionResult<Option<CallInfo>> {
        let class_hash = self.class_hash();
        match &self.tx {
            starknet_api::transaction::DeclareTransaction::V0(_)
            | starknet_api::transaction::DeclareTransaction::V1(_) => {
                if context.tx_context.block_context.versioned_constants.disable_cairo0_redeclaration
                {
                    self.try_declare(state, class_hash, None)?
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
            }) => self.try_declare(state, class_hash, Some(*compiled_class_hash))?,
        }
        Ok(None)
    }
}

impl TransactionInfoCreator for DeclareTransaction {
    fn create_tx_info(&self) -> TransactionInfo {
        // TODO(Nir, 01/11/2023): Consider to move this (from all get_tx_info methods).
        let common_fields = CommonAccountFields {
            transaction_hash: self.tx_hash(),
            version: self.version(),
            signature: self.signature(),
            nonce: self.nonce(),
            sender_address: self.sender_address(),
            only_query: self.only_query,
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
#[derive(Debug, Clone)]
pub struct DeployAccountTransaction {
    pub tx: starknet_api::executable_transaction::DeployAccountTransaction,
    // Indicates the presence of the only_query bit in the version.
    pub only_query: bool,
}

impl DeployAccountTransaction {
    pub fn new(
        deploy_account_tx: starknet_api::transaction::DeployAccountTransaction,
        tx_hash: TransactionHash,
        contract_address: ContractAddress,
    ) -> Self {
        Self {
            tx: starknet_api::executable_transaction::DeployAccountTransaction {
                tx: deploy_account_tx,
                tx_hash,
                contract_address,
            },
            only_query: false,
        }
    }

    pub fn new_for_query(
        deploy_account_tx: starknet_api::transaction::DeployAccountTransaction,
        tx_hash: TransactionHash,
        contract_address: ContractAddress,
    ) -> Self {
        Self {
            tx: starknet_api::executable_transaction::DeployAccountTransaction {
                tx: deploy_account_tx,
                tx_hash,
                contract_address,
            },
            only_query: true,
        }
    }

    implement_inner_tx_getter_calls!(
        (class_hash, ClassHash),
        (constructor_calldata, Calldata),
        (contract_address, ContractAddress),
        (contract_address_salt, ContractAddressSalt),
        (nonce, Nonce),
        (signature, TransactionSignature),
        (tx_hash, TransactionHash),
        (version, TransactionVersion)
    );

    pub fn tx(&self) -> &starknet_api::transaction::DeployAccountTransaction {
        self.tx.tx()
    }
}

impl<S: State> Executable<S> for DeployAccountTransaction {
    fn run_execute(
        &self,
        state: &mut S,
        resources: &mut ExecutionResources,
        context: &mut EntryPointExecutionContext,
        remaining_gas: &mut u64,
    ) -> TransactionExecutionResult<Option<CallInfo>> {
        let class_hash = self.class_hash();
        let ctor_context = ConstructorContext {
            class_hash,
            code_address: None,
            storage_address: self.contract_address(),
            caller_address: ContractAddress::default(),
        };
        let call_info = execute_deployment(
            state,
            resources,
            context,
            ctor_context,
            self.constructor_calldata(),
            remaining_gas,
        )?;

        Ok(Some(call_info))
    }
}

impl TransactionInfoCreator for DeployAccountTransaction {
    fn create_tx_info(&self) -> TransactionInfo {
        let common_fields = CommonAccountFields {
            transaction_hash: self.tx_hash(),
            version: self.version(),
            signature: self.signature(),
            nonce: self.nonce(),
            sender_address: self.contract_address(),
            only_query: self.only_query,
        };

        match &self.tx() {
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

#[derive(Debug, Clone)]
pub struct InvokeTransaction {
    pub tx: starknet_api::executable_transaction::InvokeTransaction,
    // Indicates the presence of the only_query bit in the version.
    pub only_query: bool,
}

impl InvokeTransaction {
    pub fn new(
        invoke_tx: starknet_api::transaction::InvokeTransaction,
        tx_hash: TransactionHash,
    ) -> Self {
        Self {
            tx: starknet_api::executable_transaction::InvokeTransaction { tx: invoke_tx, tx_hash },
            only_query: false,
        }
    }

    pub fn new_for_query(
        invoke_tx: starknet_api::transaction::InvokeTransaction,
        tx_hash: TransactionHash,
    ) -> Self {
        Self {
            tx: starknet_api::executable_transaction::InvokeTransaction { tx: invoke_tx, tx_hash },
            only_query: true,
        }
    }

    implement_inner_tx_getter_calls!(
        (calldata, Calldata),
        (nonce, Nonce),
        (signature, TransactionSignature),
        (sender_address, ContractAddress),
        (tx_hash, TransactionHash),
        (version, TransactionVersion)
    );

    pub fn tx(&self) -> &starknet_api::transaction::InvokeTransaction {
        self.tx.tx()
    }
}

impl<S: State> Executable<S> for InvokeTransaction {
    fn run_execute(
        &self,
        state: &mut S,
        resources: &mut ExecutionResources,
        context: &mut EntryPointExecutionContext,
        remaining_gas: &mut u64,
    ) -> TransactionExecutionResult<Option<CallInfo>> {
        let entry_point_selector = match &self.tx.tx {
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

        let call_info = execute_call
            .non_reverting_execute(state, resources, context, remaining_gas)
            .map_err(|error| TransactionExecutionError::ExecutionError {
                error,
                class_hash,
                storage_address,
                selector: entry_point_selector,
            })?;

        Ok(Some(call_info))
    }
}

impl TransactionInfoCreator for InvokeTransaction {
    fn create_tx_info(&self) -> TransactionInfo {
        let common_fields = CommonAccountFields {
            transaction_hash: self.tx_hash(),
            version: self.version(),
            signature: self.signature(),
            nonce: self.nonce(),
            sender_address: self.sender_address(),
            only_query: self.only_query,
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

impl HasRelatedFeeType for L1HandlerTransaction {
    fn version(&self) -> TransactionVersion {
        self.tx.version
    }

    fn is_l1_handler(&self) -> bool {
        true
    }
}

impl<S: State> Executable<S> for L1HandlerTransaction {
    fn run_execute(
        &self,
        state: &mut S,
        resources: &mut ExecutionResources,
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

        execute_call
            .non_reverting_execute(state, resources, context, remaining_gas)
            .map(Some)
            .map_err(|error| TransactionExecutionError::ExecutionError {
                error,
                class_hash,
                storage_address,
                selector,
            })
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

impl<S: State> Executable<S> for ExecutableDeclareTx {
    fn run_execute(
        &self,
        state: &mut S,
        _resources: &mut ExecutionResources,
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
                    state.set_contract_class(class_hash, contract_class(self).try_into()?)?;
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

impl TransactionInfoCreator for ExecutableDeclareTx {
    fn create_tx_info(&self) -> TransactionInfo {
        // TODO(Nir, 01/11/2023): Consider to move this (from all get_tx_info methods).
        let common_fields = CommonAccountFields {
            transaction_hash: self.tx_hash,
            version: self.version(),
            signature: self.signature(),
            nonce: self.nonce(),
            sender_address: self.sender_address(),
            only_query: false, /* Reminder(AvivG remove before PR): override by the AccountTx
                                * when calling. */
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

impl<S: State> Executable<S> for ExecutableDeployAccountTx {
    fn run_execute(
        &self,
        state: &mut S,
        resources: &mut ExecutionResources,
        context: &mut EntryPointExecutionContext,
        remaining_gas: &mut u64,
    ) -> TransactionExecutionResult<Option<CallInfo>> {
        let class_hash = self.class_hash();
        let ctor_context = ConstructorContext {
            class_hash,
            code_address: None,
            storage_address: self.contract_address(),
            caller_address: ContractAddress::default(),
        };
        let call_info = execute_deployment(
            state,
            resources,
            context,
            ctor_context,
            self.constructor_calldata(),
            remaining_gas,
        )?;

        Ok(Some(call_info))
    }
}

impl TransactionInfoCreator for ExecutableDeployAccountTx {
    fn create_tx_info(&self) -> TransactionInfo {
        let common_fields = CommonAccountFields {
            transaction_hash: self.tx_hash(),
            version: self.version(),
            signature: self.signature(),
            nonce: self.nonce(),
            sender_address: self.contract_address(),
            only_query: false, /* Reminder(AvivG remove before PR): override by the AccountTx
                                * when calling. */
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

impl<S: State> Executable<S> for ExecutableInvokeTx {
    fn run_execute(
        &self,
        state: &mut S,
        resources: &mut ExecutionResources,
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

        let call_info = execute_call
            .non_reverting_execute(state, resources, context, remaining_gas)
            .map_err(|error| TransactionExecutionError::ExecutionError {
                error,
                class_hash,
                storage_address,
                selector: entry_point_selector,
            })?;
        Ok(Some(call_info))
    }
}

impl TransactionInfoCreator for ExecutableInvokeTx {
    fn create_tx_info(&self) -> TransactionInfo {
        let common_fields = CommonAccountFields {
            transaction_hash: self.tx_hash(),
            version: self.version(),
            signature: self.signature(),
            nonce: self.nonce(),
            sender_address: self.sender_address(),
            only_query: false, /* Reminder(AvivG remove before PR): override by the AccountTx
                                * when calling. */
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

fn try_declare<S: State>(
    tx: &ExecutableDeclareTx,
    state: &mut S,
    class_hash: ClassHash,
    compiled_class_hash: Option<CompiledClassHash>,
) -> TransactionExecutionResult<()> {
    match state.get_compiled_contract_class(class_hash) {
        Err(StateError::UndeclaredClassHash(_)) => {
            // Class is undeclared; declare it.
            state.set_contract_class(class_hash, contract_class(tx).try_into()?)?;
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

fn contract_class(tx: &ExecutableDeclareTx) -> ContractClass {
    tx.class_info.contract_class.clone()
}
