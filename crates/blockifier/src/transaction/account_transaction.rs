use std::sync::Arc;

use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use starknet_api::block::GasPriceVector;
use starknet_api::calldata;
use starknet_api::contract_class::EntryPointType;
use starknet_api::core::{ClassHash, ContractAddress, EntryPointSelector, Nonce};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::transaction::Resource::{L1DataGas, L1Gas, L2Gas};
use starknet_api::transaction::{
    AccountDeploymentData,
    AllResourceBounds,
    Calldata,
    Fee,
    PaymasterData,
    Tip,
    TransactionHash,
    TransactionSignature,
    TransactionVersion,
    ValidResourceBounds,
};
use starknet_types_core::felt::Felt;

use crate::abi::abi_utils::selector_from_name;
use crate::context::{BlockContext, TransactionContext};
use crate::execution::call_info::{CallInfo, Retdata};
use crate::execution::contract_class::ContractClass;
use crate::execution::entry_point::{CallEntryPoint, CallType, EntryPointExecutionContext};
use crate::execution::execution_utils::update_remaining_gas;
use crate::fee::fee_checks::{FeeCheckReportFields, PostExecutionReport};
use crate::fee::fee_utils::{
    get_fee_by_gas_vector,
    get_sequencer_balance_keys,
    verify_can_pay_committed_bounds,
};
use crate::fee::gas_usage::estimate_minimal_gas_vector;
use crate::fee::receipt::TransactionReceipt;
use crate::retdata;
use crate::state::cached_state::{StateChanges, TransactionalState};
use crate::state::state_api::{State, StateReader, UpdatableState};
use crate::transaction::constants;
use crate::transaction::errors::{
    TransactionExecutionError,
    TransactionFeeError,
    TransactionPreValidationError,
};
use crate::transaction::objects::{
    DeprecatedTransactionInfo,
    HasRelatedFeeType,
    TransactionExecutionInfo,
    TransactionExecutionResult,
    TransactionInfo,
    TransactionInfoCreator,
    TransactionPreValidationResult,
};
use crate::transaction::transaction_types::TransactionType;
use crate::transaction::transactions::{
    DeclareTransaction,
    DeployAccountTransaction,
    Executable,
    ExecutableTransaction,
    ExecutionFlags,
    InvokeTransaction,
    ValidatableTransaction,
};

#[cfg(test)]
#[path = "account_transactions_test.rs"]
mod test;

#[cfg(test)]
#[path = "execution_flavors_test.rs"]
mod flavors_test;

#[cfg(test)]
#[path = "post_execution_test.rs"]
mod post_execution_test;

/// Represents a paid Starknet transaction.
#[derive(Clone, Debug, derive_more::From)]
pub enum AccountTransaction {
    Declare(DeclareTransaction),
    DeployAccount(DeployAccountTransaction),
    Invoke(InvokeTransaction),
}

macro_rules! implement_account_tx_inner_getters {
    ($(($field:ident, $field_type:ty)),*) => {
        $(pub fn $field(&self) -> $field_type {
            match self {
                Self::Declare(tx) => tx.tx.$field().clone(),
                Self::DeployAccount(tx) => tx.tx.$field().clone(),
                Self::Invoke(tx) => tx.tx.$field().clone(),
            }
        })*
    };
}

impl TryFrom<&starknet_api::executable_transaction::Transaction> for AccountTransaction {
    type Error = TransactionExecutionError;

    fn try_from(
        value: &starknet_api::executable_transaction::Transaction,
    ) -> Result<Self, Self::Error> {
        match value {
            starknet_api::executable_transaction::Transaction::Declare(declare_tx) => {
                Ok(Self::Declare(declare_tx.clone().try_into()?))
            }
            starknet_api::executable_transaction::Transaction::DeployAccount(deploy_account_tx) => {
                Ok(Self::DeployAccount(DeployAccountTransaction {
                    tx: deploy_account_tx.clone(),
                    only_query: false,
                }))
            }
            starknet_api::executable_transaction::Transaction::Invoke(invoke_tx) => {
                Ok(Self::Invoke(InvokeTransaction { tx: invoke_tx.clone(), only_query: false }))
            }
        }
    }
}

impl TryFrom<starknet_api::executable_transaction::Transaction> for AccountTransaction {
    type Error = TransactionExecutionError;

    fn try_from(
        executable_transaction: starknet_api::executable_transaction::Transaction,
    ) -> Result<Self, Self::Error> {
        match executable_transaction {
            starknet_api::executable_transaction::Transaction::Declare(declare_tx) => {
                Ok(Self::Declare(declare_tx.try_into()?))
            }
            starknet_api::executable_transaction::Transaction::DeployAccount(deploy_account_tx) => {
                Ok(Self::DeployAccount(DeployAccountTransaction {
                    tx: deploy_account_tx,
                    only_query: false,
                }))
            }
            starknet_api::executable_transaction::Transaction::Invoke(invoke_tx) => {
                Ok(Self::Invoke(InvokeTransaction { tx: invoke_tx, only_query: false }))
            }
        }
    }
}

impl HasRelatedFeeType for AccountTransaction {
    fn version(&self) -> TransactionVersion {
        match self {
            Self::Declare(tx) => tx.tx.version(),
            Self::DeployAccount(tx) => tx.tx.version(),
            Self::Invoke(tx) => tx.tx.version(),
        }
    }

    fn is_l1_handler(&self) -> bool {
        false
    }
}

impl AccountTransaction {
    implement_account_tx_inner_getters!(
        (signature, TransactionSignature),
        (nonce, Nonce),
        (resource_bounds, ValidResourceBounds),
        (tip, Tip),
        (nonce_data_availability_mode, DataAvailabilityMode),
        (fee_data_availability_mode, DataAvailabilityMode),
        (paymaster_data, PaymasterData)
    );

    pub fn sender_address(&self) -> ContractAddress {
        match self {
            Self::Declare(tx) => tx.tx.sender_address(),
            Self::DeployAccount(tx) => tx.tx.contract_address(),
            Self::Invoke(tx) => tx.tx.sender_address(),
        }
    }

    pub fn class_hash(&self) -> Option<ClassHash> {
        match self {
            Self::Declare(tx) => Some(tx.tx.class_hash()),
            Self::DeployAccount(tx) => Some(tx.tx.class_hash()),
            Self::Invoke(_) => None,
        }
    }

    pub fn account_deployment_data(&self) -> Option<AccountDeploymentData> {
        match self {
            Self::Declare(tx) => Some(tx.tx.account_deployment_data().clone()),
            Self::DeployAccount(_) => None,
            Self::Invoke(tx) => Some(tx.tx.account_deployment_data().clone()),
        }
    }

    // TODO(nir, 01/11/2023): Consider instantiating CommonAccountFields in AccountTransaction.
    pub fn tx_type(&self) -> TransactionType {
        match self {
            AccountTransaction::Declare(_) => TransactionType::Declare,
            AccountTransaction::DeployAccount(_) => TransactionType::DeployAccount,
            AccountTransaction::Invoke(_) => TransactionType::InvokeFunction,
        }
    }

    fn validate_entry_point_selector(&self) -> EntryPointSelector {
        let validate_entry_point_name = match self {
            Self::Declare(_) => constants::VALIDATE_DECLARE_ENTRY_POINT_NAME,
            Self::DeployAccount(_) => constants::VALIDATE_DEPLOY_ENTRY_POINT_NAME,
            Self::Invoke(_) => constants::VALIDATE_ENTRY_POINT_NAME,
        };
        selector_from_name(validate_entry_point_name)
    }

    // Calldata for validation contains transaction fields that cannot be obtained by calling
    // `et_tx_info()`.
    fn validate_entrypoint_calldata(&self) -> Calldata {
        match self {
            Self::Declare(tx) => calldata![tx.class_hash().0],
            Self::DeployAccount(tx) => Calldata(
                [
                    vec![tx.class_hash().0, tx.contract_address_salt().0],
                    (*tx.constructor_calldata().0).clone(),
                ]
                .concat()
                .into(),
            ),
            // Calldata for validation is the same calldata as for the execution itself.
            Self::Invoke(tx) => tx.calldata(),
        }
    }

    pub fn calldata_length(&self) -> usize {
        let calldata = match self {
            Self::Declare(_tx) => calldata![],
            Self::DeployAccount(tx) => tx.constructor_calldata(),
            Self::Invoke(tx) => tx.calldata(),
        };

        calldata.0.len()
    }

    pub fn signature_length(&self) -> usize {
        let signature = match self {
            Self::Declare(tx) => tx.signature(),
            Self::DeployAccount(tx) => tx.signature(),
            Self::Invoke(tx) => tx.signature(),
        };

        signature.0.len()
    }

    pub fn tx_hash(&self) -> TransactionHash {
        match self {
            Self::Declare(tx) => tx.tx_hash(),
            Self::DeployAccount(tx) => tx.tx_hash(),
            Self::Invoke(tx) => tx.tx_hash(),
        }
    }

    pub fn enforce_fee(&self) -> bool {
        self.create_tx_info().enforce_fee()
    }

    fn verify_tx_version(&self, version: TransactionVersion) -> TransactionExecutionResult<()> {
        let allowed_versions: Vec<TransactionVersion> = match self {
            // Support `Declare` of version 0 in order to allow bootstrapping of a new system.
            Self::Declare(_) => {
                vec![
                    TransactionVersion::ZERO,
                    TransactionVersion::ONE,
                    TransactionVersion::TWO,
                    TransactionVersion::THREE,
                ]
            }
            Self::DeployAccount(_) => {
                vec![TransactionVersion::ONE, TransactionVersion::THREE]
            }
            Self::Invoke(_) => {
                vec![TransactionVersion::ZERO, TransactionVersion::ONE, TransactionVersion::THREE]
            }
        };
        if allowed_versions.contains(&version) {
            Ok(())
        } else {
            Err(TransactionExecutionError::InvalidVersion { version, allowed_versions })
        }
    }

    // Performs static checks before executing validation entry point.
    // Note that nonce is incremented during these checks.
    pub fn perform_pre_validation_stage<S: State + StateReader>(
        &self,
        state: &mut S,
        tx_context: &TransactionContext,
        charge_fee: bool,
        strict_nonce_check: bool,
    ) -> TransactionPreValidationResult<()> {
        let tx_info = &tx_context.tx_info;
        Self::handle_nonce(state, tx_info, strict_nonce_check)?;

        if charge_fee {
            self.check_fee_bounds(tx_context)?;

            verify_can_pay_committed_bounds(state, tx_context)?;
        }

        Ok(())
    }

    fn check_fee_bounds(
        &self,
        tx_context: &TransactionContext,
    ) -> TransactionPreValidationResult<()> {
        // TODO(Aner): seprate to cases based on context.resource_bounds type
        let minimal_gas_amount_vector = estimate_minimal_gas_vector(
            &tx_context.block_context,
            self,
            &tx_context.get_gas_vector_computation_mode(),
        );
        let TransactionContext { block_context, tx_info } = tx_context;
        let block_info = &block_context.block_info;
        let fee_type = &tx_info.fee_type();
        match tx_info {
            TransactionInfo::Current(context) => {
                let resources_amount_tuple = match &context.resource_bounds {
                    ValidResourceBounds::L1Gas(l1_gas_resource_bounds) => vec![(
                        L1Gas,
                        l1_gas_resource_bounds,
                        minimal_gas_amount_vector.to_discounted_l1_gas(tx_context.get_gas_prices()),
                        block_info.gas_prices.get_l1_gas_price_by_fee_type(fee_type),
                    )],
                    ValidResourceBounds::AllResources(AllResourceBounds {
                        l1_gas: l1_gas_resource_bounds,
                        l2_gas: l2_gas_resource_bounds,
                        l1_data_gas: l1_data_gas_resource_bounds,
                    }) => {
                        let GasPriceVector { l1_gas_price, l1_data_gas_price, l2_gas_price } =
                            block_info.gas_prices.get_gas_prices_by_fee_type(fee_type);
                        vec![
                            (
                                L1Gas,
                                l1_gas_resource_bounds,
                                minimal_gas_amount_vector.l1_gas,
                                *l1_gas_price,
                            ),
                            (
                                L1DataGas,
                                l1_data_gas_resource_bounds,
                                minimal_gas_amount_vector.l1_data_gas,
                                *l1_data_gas_price,
                            ),
                            (
                                L2Gas,
                                l2_gas_resource_bounds,
                                minimal_gas_amount_vector.l2_gas,
                                *l2_gas_price,
                            ),
                        ]
                    }
                };
                for (resource, resource_bounds, minimal_gas_amount, actual_gas_price) in
                    resources_amount_tuple
                {
                    // TODO(Aner): refactor to indicate both amount and price are too low.
                    // TODO(Aner): refactor to return all amounts that are too low.
                    if minimal_gas_amount > resource_bounds.max_amount {
                        return Err(TransactionFeeError::MaxGasAmountTooLow {
                            resource,
                            max_gas_amount: resource_bounds.max_amount,
                            minimal_gas_amount,
                        })?;
                    }
                    // TODO(Aner): refactor to return all prices that are too low.
                    if resource_bounds.max_price_per_unit < actual_gas_price.get() {
                        return Err(TransactionFeeError::MaxGasPriceTooLow {
                            resource,
                            max_gas_price: resource_bounds.max_price_per_unit,
                            actual_gas_price: actual_gas_price.into(),
                        })?;
                    }
                }
            }
            TransactionInfo::Deprecated(context) => {
                let max_fee = context.max_fee;
                let min_fee =
                    get_fee_by_gas_vector(block_info, minimal_gas_amount_vector, fee_type);
                if max_fee < min_fee {
                    return Err(TransactionFeeError::MaxFeeTooLow { min_fee, max_fee })?;
                }
            }
        };
        Ok(())
    }

    fn handle_nonce(
        state: &mut dyn State,
        tx_info: &TransactionInfo,
        strict: bool,
    ) -> TransactionPreValidationResult<()> {
        if tx_info.is_v0() {
            return Ok(());
        }

        let address = tx_info.sender_address();
        let account_nonce = state.get_nonce_at(address)?;
        let incoming_tx_nonce = tx_info.nonce();
        let valid_nonce = if strict {
            account_nonce == incoming_tx_nonce
        } else {
            account_nonce <= incoming_tx_nonce
        };
        if valid_nonce {
            return Ok(state.increment_nonce(address)?);
        }
        Err(TransactionPreValidationError::InvalidNonce {
            address,
            account_nonce,
            incoming_tx_nonce,
        })
    }

    fn handle_validate_tx(
        &self,
        state: &mut dyn State,
        resources: &mut ExecutionResources,
        tx_context: Arc<TransactionContext>,
        remaining_gas: &mut u64,
        validate: bool,
        limit_steps_by_resources: bool,
    ) -> TransactionExecutionResult<Option<CallInfo>> {
        if validate {
            self.validate_tx(state, resources, tx_context, remaining_gas, limit_steps_by_resources)
        } else {
            Ok(None)
        }
    }

    fn assert_actual_fee_in_bounds(tx_context: &Arc<TransactionContext>, actual_fee: Fee) {
        match &tx_context.tx_info {
            TransactionInfo::Current(context) => {
                let max_fee = context.resource_bounds.max_possible_fee();
                if actual_fee > max_fee {
                    panic!(
                        "Actual fee {:#?} exceeded bounds; max possible fee is {:#?} (computed \
                         from {:#?}).",
                        actual_fee, max_fee, context.resource_bounds
                    );
                }
            }
            TransactionInfo::Deprecated(DeprecatedTransactionInfo { max_fee, .. }) => {
                if actual_fee > *max_fee {
                    panic!(
                        "Actual fee {:#?} exceeded bounds; max fee is {:#?}.",
                        actual_fee, max_fee
                    );
                }
            }
        }
    }

    fn handle_fee<S: StateReader>(
        state: &mut TransactionalState<'_, S>,
        tx_context: Arc<TransactionContext>,
        actual_fee: Fee,
        charge_fee: bool,
        concurrency_mode: bool,
    ) -> TransactionExecutionResult<Option<CallInfo>> {
        if !charge_fee || actual_fee == Fee(0) {
            // Fee charging is not enforced in some transaction simulations and tests.
            return Ok(None);
        }

        Self::assert_actual_fee_in_bounds(&tx_context, actual_fee);

        let fee_transfer_call_info = if concurrency_mode && !tx_context.is_sequencer_the_sender() {
            Self::concurrency_execute_fee_transfer(state, tx_context, actual_fee)?
        } else {
            Self::execute_fee_transfer(state, tx_context, actual_fee)?
        };

        Ok(Some(fee_transfer_call_info))
    }

    fn execute_fee_transfer(
        state: &mut dyn State,
        tx_context: Arc<TransactionContext>,
        actual_fee: Fee,
    ) -> TransactionExecutionResult<CallInfo> {
        // The least significant 128 bits of the amount transferred.
        let lsb_amount = Felt::from(actual_fee.0);
        // The most significant 128 bits of the amount transferred.
        let msb_amount = Felt::from(0_u8);

        let TransactionContext { block_context, tx_info } = tx_context.as_ref();
        let storage_address = tx_context.fee_token_address();
        let fee_transfer_call = CallEntryPoint {
            class_hash: None,
            code_address: None,
            entry_point_type: EntryPointType::External,
            entry_point_selector: selector_from_name(constants::TRANSFER_ENTRY_POINT_NAME),
            calldata: calldata![
                *block_context.block_info.sequencer_address.0.key(), // Recipient.
                lsb_amount,
                msb_amount
            ],
            storage_address,
            caller_address: tx_info.sender_address(),
            call_type: CallType::Call,
            // The fee-token contract is a Cairo 0 contract, hence the initial gas is irrelevant.
            initial_gas: block_context
                .versioned_constants
                .os_constants
                .gas_costs
                .default_initial_gas_cost,
        };

        let mut context = EntryPointExecutionContext::new_invoke(tx_context, true);

        Ok(fee_transfer_call
            .execute(state, &mut ExecutionResources::default(), &mut context)
            .map_err(TransactionFeeError::ExecuteFeeTransferError)?)
    }

    /// Handles fee transfer in concurrent execution.
    ///
    /// Accessing and updating the sequencer balance at this stage is a bottleneck; this function
    /// manipulates the state to avoid that part.
    /// Note: the returned transfer call info is partial, and should be completed at the commit
    /// stage, as well as the actual sequencer balance.
    fn concurrency_execute_fee_transfer<S: StateReader>(
        state: &mut TransactionalState<'_, S>,
        tx_context: Arc<TransactionContext>,
        actual_fee: Fee,
    ) -> TransactionExecutionResult<CallInfo> {
        let fee_address = tx_context.fee_token_address();
        let (sequencer_balance_key_low, sequencer_balance_key_high) =
            get_sequencer_balance_keys(&tx_context.block_context);
        let mut transfer_state = TransactionalState::create_transactional(state);

        // Set the initial sequencer balance to avoid tarnishing the read-set of the transaction.
        let cache = transfer_state.cache.get_mut();
        for key in [sequencer_balance_key_low, sequencer_balance_key_high] {
            cache.set_storage_initial_value(fee_address, key, Felt::ZERO);
        }

        let fee_transfer_call_info =
            AccountTransaction::execute_fee_transfer(&mut transfer_state, tx_context, actual_fee);
        // Commit without updating the sequencer balance.
        let storage_writes = &mut transfer_state.cache.get_mut().writes.storage;
        storage_writes.remove(&(fee_address, sequencer_balance_key_low));
        storage_writes.remove(&(fee_address, sequencer_balance_key_high));
        transfer_state.commit();
        fee_transfer_call_info
    }

    fn run_execute<S: State>(
        &self,
        state: &mut S,
        resources: &mut ExecutionResources,
        context: &mut EntryPointExecutionContext,
        remaining_gas: &mut u64,
    ) -> TransactionExecutionResult<Option<CallInfo>> {
        match &self {
            Self::Declare(tx) => tx.run_execute(state, resources, context, remaining_gas),
            Self::DeployAccount(tx) => tx.run_execute(state, resources, context, remaining_gas),
            Self::Invoke(tx) => tx.run_execute(state, resources, context, remaining_gas),
        }
    }

    fn run_non_revertible<S: StateReader>(
        &self,
        state: &mut TransactionalState<'_, S>,
        tx_context: Arc<TransactionContext>,
        remaining_gas: &mut u64,
        validate: bool,
        charge_fee: bool,
    ) -> TransactionExecutionResult<ValidateExecuteCallInfo> {
        let mut resources = ExecutionResources::default();
        let validate_call_info: Option<CallInfo>;
        let execute_call_info: Option<CallInfo>;
        if matches!(self, Self::DeployAccount(_)) {
            // Handle `DeployAccount` transactions separately, due to different order of things.
            // Also, the execution context required form the `DeployAccount` execute phase is
            // validation context.
            let mut execution_context =
                EntryPointExecutionContext::new_validate(tx_context.clone(), charge_fee);
            execute_call_info =
                self.run_execute(state, &mut resources, &mut execution_context, remaining_gas)?;
            validate_call_info = self.handle_validate_tx(
                state,
                &mut resources,
                tx_context.clone(),
                remaining_gas,
                validate,
                charge_fee,
            )?;
        } else {
            let mut execution_context =
                EntryPointExecutionContext::new_invoke(tx_context.clone(), charge_fee);
            validate_call_info = self.handle_validate_tx(
                state,
                &mut resources,
                tx_context.clone(),
                remaining_gas,
                validate,
                charge_fee,
            )?;
            execute_call_info =
                self.run_execute(state, &mut resources, &mut execution_context, remaining_gas)?;
        }

        let tx_receipt = TransactionReceipt::from_account_tx(
            self,
            &tx_context,
            &state.get_actual_state_changes()?,
            &resources,
            CallInfo::summarize_many(validate_call_info.iter().chain(execute_call_info.iter())),
            0,
        );

        let post_execution_report =
            PostExecutionReport::new(state, &tx_context, &tx_receipt, charge_fee)?;
        match post_execution_report.error() {
            Some(error) => Err(error.into()),
            None => Ok(ValidateExecuteCallInfo::new_accepted(
                validate_call_info,
                execute_call_info,
                tx_receipt,
            )),
        }
    }

    fn run_revertible<S: StateReader>(
        &self,
        state: &mut TransactionalState<'_, S>,
        tx_context: Arc<TransactionContext>,
        remaining_gas: &mut u64,
        validate: bool,
        charge_fee: bool,
    ) -> TransactionExecutionResult<ValidateExecuteCallInfo> {
        let mut resources = ExecutionResources::default();
        let mut execution_context =
            EntryPointExecutionContext::new_invoke(tx_context.clone(), charge_fee);
        // Run the validation, and if execution later fails, only keep the validation diff.
        let validate_call_info = self.handle_validate_tx(
            state,
            &mut resources,
            tx_context.clone(),
            remaining_gas,
            validate,
            charge_fee,
        )?;

        let n_allotted_execution_steps = execution_context.subtract_validation_and_overhead_steps(
            &validate_call_info,
            &self.tx_type(),
            self.calldata_length(),
        );

        // Save the state changes resulting from running `validate_tx`, to be used later for
        // resource and fee calculation.
        let validate_state_changes = state.get_actual_state_changes()?;

        // Create copies of state and resources for the execution.
        // Both will be rolled back if the execution is reverted or committed upon success.
        let mut execution_resources = resources.clone();
        let mut execution_state = TransactionalState::create_transactional(state);

        let execution_result = self.run_execute(
            &mut execution_state,
            &mut execution_resources,
            &mut execution_context,
            remaining_gas,
        );

        // Pre-compute cost in case of revert.
        let execution_steps_consumed =
            n_allotted_execution_steps - execution_context.n_remaining_steps();
        let revert_cost = TransactionReceipt::from_account_tx(
            self,
            &tx_context,
            &validate_state_changes,
            &resources,
            CallInfo::summarize_many(validate_call_info.iter()),
            execution_steps_consumed,
        );

        match execution_result {
            Ok(execute_call_info) => {
                // When execution succeeded, calculate the actual required fee before committing the
                // transactional state. If max_fee is insufficient, revert the `run_execute` part.
                let tx_receipt = TransactionReceipt::from_account_tx(
                    self,
                    &tx_context,
                    &StateChanges::merge(vec![
                        validate_state_changes,
                        execution_state.get_actual_state_changes()?,
                    ]),
                    &execution_resources,
                    CallInfo::summarize_many(
                        validate_call_info.iter().chain(execute_call_info.iter()),
                    ),
                    0,
                );
                // Post-execution checks.
                let post_execution_report = PostExecutionReport::new(
                    &mut execution_state,
                    &tx_context,
                    &tx_receipt,
                    charge_fee,
                )?;
                match post_execution_report.error() {
                    Some(post_execution_error) => {
                        // Post-execution check failed. Revert the execution, compute the final fee
                        // to charge and recompute resources used (to be consistent with other
                        // revert case, compute resources by adding consumed execution steps to
                        // validation resources).
                        execution_state.abort();
                        Ok(ValidateExecuteCallInfo::new_reverted(
                            validate_call_info,
                            post_execution_error.to_string(),
                            TransactionReceipt {
                                fee: post_execution_report.recommended_fee(),
                                ..revert_cost
                            },
                        ))
                    }
                    None => {
                        // Post-execution check passed, commit the execution.
                        execution_state.commit();
                        Ok(ValidateExecuteCallInfo::new_accepted(
                            validate_call_info,
                            execute_call_info,
                            tx_receipt,
                        ))
                    }
                }
            }
            Err(execution_error) => {
                // Error during execution. Revert, even if the error is sequencer-related.
                execution_state.abort();
                let post_execution_report =
                    PostExecutionReport::new(state, &tx_context, &revert_cost, charge_fee)?;
                Ok(ValidateExecuteCallInfo::new_reverted(
                    validate_call_info,
                    execution_error.to_string(),
                    TransactionReceipt {
                        fee: post_execution_report.recommended_fee(),
                        ..revert_cost
                    },
                ))
            }
        }
    }

    /// Returns 0 on non-declare transactions; for declare transactions, returns the class code
    /// size.
    pub(crate) fn declare_code_size(&self) -> usize {
        if let Self::Declare(tx) = self { tx.class_info.code_size() } else { 0 }
    }

    fn is_non_revertible(&self, tx_info: &TransactionInfo) -> bool {
        // Reverting a Declare or Deploy transaction is not currently supported in the OS.
        match self {
            Self::Declare(_) => true,
            Self::DeployAccount(_) => true,
            Self::Invoke(_) => {
                // V0 transactions do not have validation; we cannot deduct fee for execution. Thus,
                // invoke transactions of are non-revertible iff they are of version 0.
                tx_info.is_v0()
            }
        }
    }

    /// Runs validation and execution.
    fn run_or_revert<S: StateReader>(
        &self,
        state: &mut TransactionalState<'_, S>,
        remaining_gas: &mut u64,
        tx_context: Arc<TransactionContext>,
        validate: bool,
        charge_fee: bool,
    ) -> TransactionExecutionResult<ValidateExecuteCallInfo> {
        if self.is_non_revertible(&tx_context.tx_info) {
            return self.run_non_revertible(state, tx_context, remaining_gas, validate, charge_fee);
        }

        self.run_revertible(state, tx_context, remaining_gas, validate, charge_fee)
    }
}

impl<U: UpdatableState> ExecutableTransaction<U> for AccountTransaction {
    fn execute_raw(
        &self,
        state: &mut TransactionalState<'_, U>,
        block_context: &BlockContext,
        execution_flags: ExecutionFlags,
    ) -> TransactionExecutionResult<TransactionExecutionInfo> {
        let tx_context = Arc::new(block_context.to_tx_context(self));
        self.verify_tx_version(tx_context.tx_info.version())?;

        // Nonce and fee check should be done before running user code.
        let strict_nonce_check = true;
        self.perform_pre_validation_stage(
            state,
            &tx_context,
            execution_flags.charge_fee,
            strict_nonce_check,
        )?;

        // Run validation and execution.
        let mut remaining_gas = block_context.versioned_constants.tx_default_initial_gas();
        let ValidateExecuteCallInfo {
            validate_call_info,
            execute_call_info,
            revert_error,
            final_cost:
                TransactionReceipt {
                    fee: final_fee,
                    da_gas: final_da_gas,
                    resources: final_resources,
                    gas: total_gas,
                },
        } = self.run_or_revert(
            state,
            &mut remaining_gas,
            tx_context.clone(),
            execution_flags.validate,
            execution_flags.charge_fee,
        )?;
        let fee_transfer_call_info = Self::handle_fee(
            state,
            tx_context,
            final_fee,
            execution_flags.charge_fee,
            execution_flags.concurrency_mode,
        )?;

        let tx_execution_info = TransactionExecutionInfo {
            validate_call_info,
            execute_call_info,
            fee_transfer_call_info,
            receipt: TransactionReceipt {
                fee: final_fee,
                da_gas: final_da_gas,
                resources: final_resources,
                gas: total_gas,
            },
            revert_error,
        };
        Ok(tx_execution_info)
    }
}

impl TransactionInfoCreator for AccountTransaction {
    fn create_tx_info(&self) -> TransactionInfo {
        match self {
            Self::Declare(tx) => tx.create_tx_info(),
            Self::DeployAccount(tx) => tx.create_tx_info(),
            Self::Invoke(tx) => tx.create_tx_info(),
        }
    }
}

/// Represents a bundle of validate-execute stage execution effects.
struct ValidateExecuteCallInfo {
    validate_call_info: Option<CallInfo>,
    execute_call_info: Option<CallInfo>,
    revert_error: Option<String>,
    final_cost: TransactionReceipt,
}

impl ValidateExecuteCallInfo {
    pub fn new_accepted(
        validate_call_info: Option<CallInfo>,
        execute_call_info: Option<CallInfo>,
        final_cost: TransactionReceipt,
    ) -> Self {
        Self { validate_call_info, execute_call_info, revert_error: None, final_cost }
    }

    pub fn new_reverted(
        validate_call_info: Option<CallInfo>,
        revert_error: String,
        final_cost: TransactionReceipt,
    ) -> Self {
        Self {
            validate_call_info,
            execute_call_info: None,
            revert_error: Some(revert_error),
            final_cost,
        }
    }
}

impl ValidatableTransaction for AccountTransaction {
    fn validate_tx(
        &self,
        state: &mut dyn State,
        resources: &mut ExecutionResources,
        tx_context: Arc<TransactionContext>,
        remaining_gas: &mut u64,
        limit_steps_by_resources: bool,
    ) -> TransactionExecutionResult<Option<CallInfo>> {
        let mut context =
            EntryPointExecutionContext::new_validate(tx_context, limit_steps_by_resources);
        let tx_info = &context.tx_context.tx_info;
        if tx_info.is_v0() {
            return Ok(None);
        }

        let storage_address = tx_info.sender_address();
        let class_hash = state.get_class_hash_at(storage_address)?;
        let validate_selector = self.validate_entry_point_selector();
        let validate_call = CallEntryPoint {
            entry_point_type: EntryPointType::External,
            entry_point_selector: validate_selector,
            calldata: self.validate_entrypoint_calldata(),
            class_hash: None,
            code_address: None,
            storage_address,
            caller_address: ContractAddress::default(),
            call_type: CallType::Call,
            initial_gas: *remaining_gas,
        };

        // Note that we allow a revert here and we handle it bellow to get a better error message.
        let validate_call_info =
            validate_call.execute(state, resources, &mut context).map_err(|error| {
                TransactionExecutionError::ValidateTransactionError {
                    error,
                    class_hash,
                    storage_address,
                    selector: validate_selector,
                }
            })?;

        // Validate return data.
        let contract_class = state.get_compiled_contract_class(class_hash)?;
        if matches!(contract_class, ContractClass::V1(_) | ContractClass::V1Native(_)) {
            // The account contract class is a Cairo 1.0 contract; the `validate` entry point should
            // return `VALID`.
            let expected_retdata = retdata![Felt::from_hex(constants::VALIDATE_RETDATA)?];

            if validate_call_info.execution.failed {
                return Err(TransactionExecutionError::PanicInValidate {
                    panic_reason: validate_call_info.execution.retdata,
                });
            }

            if validate_call_info.execution.retdata != expected_retdata {
                return Err(TransactionExecutionError::InvalidValidateReturnData {
                    actual: validate_call_info.execution.retdata,
                });
            }
        }

        update_remaining_gas(remaining_gas, &validate_call_info);

        Ok(Some(validate_call_info))
    }
}
