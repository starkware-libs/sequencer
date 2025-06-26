use std::sync::Arc;

use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::block::GasPriceVector;
use starknet_api::calldata;
use starknet_api::contract_class::EntryPointType;
use starknet_api::core::{ClassHash, ContractAddress, EntryPointSelector, Nonce};
use starknet_api::data_availability::DataAvailabilityMode;
use starknet_api::executable_transaction::{AccountTransaction as Transaction, TransactionType};
use starknet_api::execution_resources::GasAmount;
use starknet_api::transaction::fields::Resource::{L1DataGas, L1Gas, L2Gas};
use starknet_api::transaction::fields::{
    AccountDeploymentData,
    AllResourceBounds,
    Calldata,
    Fee,
    PaymasterData,
    Tip,
    TransactionSignature,
    ValidResourceBounds,
};
use starknet_api::transaction::{constants, TransactionHash, TransactionVersion};
use starknet_types_core::felt::Felt;

use super::errors::ResourceBoundsError;
use crate::context::{BlockContext, GasCounter, TransactionContext};
use crate::execution::call_info::CallInfo;
use crate::execution::common_hints::ExecutionMode;
use crate::execution::contract_class::RunnableCompiledClass;
use crate::execution::entry_point::{
    CallEntryPoint,
    CallType,
    EntryPointExecutionContext,
    SierraGasRevertTracker,
};
use crate::execution::stack_trace::{
    extract_trailing_cairo1_revert_trace,
    gen_tx_execution_error_trace,
    Cairo1RevertHeader,
};
use crate::fee::fee_checks::{FeeCheckReportFields, PostExecutionReport};
use crate::fee::fee_utils::{
    get_fee_by_gas_vector,
    get_sequencer_balance_keys,
    verify_can_pay_committed_bounds,
    GasVectorToL1GasForFee,
};
use crate::fee::gas_usage::estimate_minimal_gas_vector;
use crate::fee::receipt::TransactionReceipt;
use crate::retdata;
use crate::state::cached_state::{StateCache, TransactionalState};
use crate::state::state_api::{State, StateReader, UpdatableState};
use crate::transaction::errors::{
    TransactionExecutionError,
    TransactionFeeError,
    TransactionPreValidationError,
};
use crate::transaction::objects::{
    HasRelatedFeeType,
    RevertError,
    TransactionExecutionInfo,
    TransactionExecutionResult,
    TransactionInfo,
    TransactionInfoCreator,
    TransactionInfoCreatorInner,
    TransactionPreValidationResult,
};
use crate::transaction::transactions::{
    enforce_fee,
    Executable,
    ExecutableTransaction,
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

#[derive(Clone, Debug, derive_more::From)]
pub struct ExecutionFlags {
    pub only_query: bool,
    pub charge_fee: bool,
    pub validate: bool,
    pub strict_nonce_check: bool,
}

impl Default for ExecutionFlags {
    fn default() -> Self {
        Self { only_query: false, charge_fee: true, validate: true, strict_nonce_check: true }
    }
}

/// Represents a paid Starknet transaction.
#[derive(Clone, Debug, derive_more::From)]
pub struct AccountTransaction {
    pub tx: Transaction,
    pub execution_flags: ExecutionFlags,
}
// TODO(AvivG): create additional macro that returns a reference.
macro_rules! implement_tx_getter_calls {
    ($(($field:ident, $field_type:ty)),*) => {
        $(pub fn $field(&self) -> $field_type {
            self.tx.$field()
        })*
    };
}

impl HasRelatedFeeType for AccountTransaction {
    fn version(&self) -> TransactionVersion {
        self.tx.version()
    }

    fn is_l1_handler(&self) -> bool {
        false
    }
}

impl AccountTransaction {
    implement_tx_getter_calls!(
        (resource_bounds, ValidResourceBounds),
        (tip, Tip),
        (sender_address, ContractAddress),
        (tx_hash, TransactionHash),
        (signature, TransactionSignature),
        (nonce, Nonce),
        (nonce_data_availability_mode, DataAvailabilityMode),
        (fee_data_availability_mode, DataAvailabilityMode),
        (paymaster_data, PaymasterData)
    );

    pub fn new_with_default_flags(tx: Transaction) -> Self {
        Self { tx, execution_flags: ExecutionFlags::default() }
    }

    pub fn new_for_sequencing(tx: Transaction) -> Self {
        let execution_flags = ExecutionFlags {
            only_query: false,
            charge_fee: enforce_fee(&tx, false),
            validate: true,
            strict_nonce_check: true,
        };
        AccountTransaction { tx, execution_flags }
    }

    pub fn class_hash(&self) -> Option<ClassHash> {
        match &self.tx {
            Transaction::Declare(tx) => Some(tx.tx.class_hash()),
            Transaction::DeployAccount(tx) => Some(tx.tx.class_hash()),
            Transaction::Invoke(_) => None,
        }
    }

    pub fn account_deployment_data(&self) -> Option<AccountDeploymentData> {
        match &self.tx {
            Transaction::Declare(tx) => Some(tx.tx.account_deployment_data().clone()),
            Transaction::DeployAccount(_) => None,
            Transaction::Invoke(tx) => Some(tx.tx.account_deployment_data().clone()),
        }
    }

    // TODO(nir, 01/11/2023): Consider instantiating CommonAccountFields in AccountTransaction.
    pub fn tx_type(&self) -> TransactionType {
        match &self.tx {
            Transaction::Declare(_) => TransactionType::Declare,
            Transaction::DeployAccount(_) => TransactionType::DeployAccount,
            Transaction::Invoke(_) => TransactionType::InvokeFunction,
        }
    }

    fn validate_entry_point_selector(&self) -> EntryPointSelector {
        let validate_entry_point_name = match &self.tx {
            Transaction::Declare(_) => constants::VALIDATE_DECLARE_ENTRY_POINT_NAME,
            Transaction::DeployAccount(_) => constants::VALIDATE_DEPLOY_ENTRY_POINT_NAME,
            Transaction::Invoke(_) => constants::VALIDATE_ENTRY_POINT_NAME,
        };
        selector_from_name(validate_entry_point_name)
    }

    // Calldata for validation contains transaction fields that cannot be obtained by calling
    // `et_tx_info()`.
    fn validate_entrypoint_calldata(&self) -> Calldata {
        match &self.tx {
            Transaction::Declare(tx) => calldata![tx.class_hash().0],
            Transaction::DeployAccount(tx) => Calldata(
                [
                    vec![tx.class_hash().0, tx.contract_address_salt().0],
                    (*tx.constructor_calldata().0).clone(),
                ]
                .concat()
                .into(),
            ),
            // Calldata for validation is the same calldata as for the execution itself.
            Transaction::Invoke(tx) => tx.calldata(),
        }
    }

    pub fn calldata_length(&self) -> usize {
        let calldata = match &self.tx {
            Transaction::Declare(_tx) => return 0,
            Transaction::DeployAccount(tx) => tx.constructor_calldata(),
            Transaction::Invoke(tx) => tx.calldata(),
        };

        calldata.0.len()
    }

    pub fn signature_length(&self) -> usize {
        self.signature().0.len()
    }

    pub fn enforce_fee(&self) -> bool {
        self.create_tx_info().enforce_fee()
    }

    fn verify_tx_version(&self, version: TransactionVersion) -> TransactionExecutionResult<()> {
        let allowed_versions: Vec<TransactionVersion> = match &self.tx {
            // Support `Declare` of version 0 in order to allow bootstrapping of a new system.
            Transaction::Declare(_) => {
                vec![
                    TransactionVersion::ZERO,
                    TransactionVersion::ONE,
                    TransactionVersion::TWO,
                    TransactionVersion::THREE,
                ]
            }
            Transaction::DeployAccount(_) => {
                vec![TransactionVersion::ONE, TransactionVersion::THREE]
            }
            Transaction::Invoke(_) => {
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
    ) -> TransactionPreValidationResult<()> {
        let tx_info = &tx_context.tx_info;
        Self::handle_nonce(state, tx_info, self.execution_flags.strict_nonce_check)?;

        if self.execution_flags.charge_fee {
            self.check_fee_bounds(tx_context)?;

            verify_can_pay_committed_bounds(state, tx_context)?;
        }

        Ok(())
    }

    fn check_fee_bounds(
        &self,
        tx_context: &TransactionContext,
    ) -> TransactionPreValidationResult<()> {
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
                        minimal_gas_amount_vector.to_l1_gas_for_fee(
                            tx_context.get_gas_prices(),
                            &tx_context.block_context.versioned_constants,
                        ),
                        block_info.gas_prices.l1_gas_price(fee_type),
                    )],
                    ValidResourceBounds::AllResources(AllResourceBounds {
                        l1_gas: l1_gas_resource_bounds,
                        l2_gas: l2_gas_resource_bounds,
                        l1_data_gas: l1_data_gas_resource_bounds,
                    }) => {
                        let GasPriceVector { l1_gas_price, l1_data_gas_price, l2_gas_price } =
                            block_info.gas_prices.gas_price_vector(fee_type);
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
                let insufficiencies = resources_amount_tuple
                    .iter()
                    .flat_map(
                        |(resource, resource_bounds, minimal_gas_amount, actual_gas_price)| {
                            let mut insufficiencies_resource = vec![];
                            if minimal_gas_amount > &resource_bounds.max_amount {
                                insufficiencies_resource.push(
                                    ResourceBoundsError::MaxGasAmountTooLow {
                                        resource: *resource,
                                        max_gas_amount: resource_bounds.max_amount,
                                        minimal_gas_amount: *minimal_gas_amount,
                                    },
                                );
                            }
                            if resource_bounds.max_price_per_unit < actual_gas_price.get() {
                                insufficiencies_resource.push(
                                    ResourceBoundsError::MaxGasPriceTooLow {
                                        resource: *resource,
                                        max_gas_price: resource_bounds.max_price_per_unit,
                                        actual_gas_price: (*actual_gas_price).into(),
                                    },
                                );
                            }
                            insufficiencies_resource
                        },
                    )
                    .collect::<Vec<_>>();
                if !insufficiencies.is_empty() {
                    return Err(TransactionFeeError::InsufficientResourceBounds {
                        errors: insufficiencies,
                    })?;
                }
            }
            TransactionInfo::Deprecated(context) => {
                let max_fee = context.max_fee;
                let min_fee = get_fee_by_gas_vector(
                    block_info,
                    minimal_gas_amount_vector,
                    fee_type,
                    tx_context.effective_tip(),
                );
                if max_fee < min_fee {
                    return Err(TransactionPreValidationError::TransactionFeeError(
                        TransactionFeeError::MaxFeeTooLow { min_fee, max_fee },
                    ));
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

    fn assert_actual_fee_in_bounds(tx_context: &Arc<TransactionContext>, actual_fee: Fee) {
        let max_fee = tx_context.max_possible_fee();
        if actual_fee > max_fee {
            match &tx_context.tx_info {
                TransactionInfo::Current(context) => {
                    panic!(
                        "Actual fee {:#?} exceeded bounds; max possible fee is {:#?} (computed \
                         from {:#?} with tip {:#?}).",
                        actual_fee,
                        max_fee,
                        context.resource_bounds,
                        tx_context.effective_tip()
                    );
                }
                TransactionInfo::Deprecated(_) => {
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
            // Fee charging is not enforced in some tests.
            // TODO(Yoni): consider setting the actual fee to zero when the flag is off.
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
        let msb_amount = Felt::ZERO;

        let TransactionContext { block_context, tx_info } = tx_context.as_ref();
        let storage_address = tx_context.fee_token_address();
        // The fee contains the cost of running this transfer, and the token contract is
        // well known to the sequencer, so there is no need to limit its run.
        let mut remaining_gas_for_fee_transfer =
            block_context.versioned_constants.os_constants.gas_costs.base.default_initial_gas_cost;
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

            initial_gas: remaining_gas_for_fee_transfer,
        };
        let mut context = EntryPointExecutionContext::new_invoke(
            tx_context,
            true,
            SierraGasRevertTracker::new(GasAmount(remaining_gas_for_fee_transfer)),
        );

        Ok(fee_transfer_call
            .execute(state, &mut context, &mut remaining_gas_for_fee_transfer)
            .map_err(|error| Box::new(TransactionFeeError::ExecuteFeeTransferError(error)))?)
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
            Self::execute_fee_transfer(&mut transfer_state, tx_context, actual_fee);
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
        context: &mut EntryPointExecutionContext,
        remaining_gas: &mut GasCounter,
    ) -> TransactionExecutionResult<Option<CallInfo>> {
        let remaining_execution_gas = &mut remaining_gas
            .limit_usage(context.tx_context.sierra_gas_limit(&context.execution_mode));
        Ok(match &self.tx {
            Transaction::Declare(tx) => tx.run_execute(state, context, remaining_execution_gas),
            Transaction::DeployAccount(tx) => {
                tx.run_execute(state, context, remaining_execution_gas)
            }
            Transaction::Invoke(tx) => tx.run_execute(state, context, remaining_execution_gas),
        }?
        .inspect(|call_info| {
            remaining_gas.subtract_used_gas(call_info);
        }))
    }

    fn run_non_revertible<S: StateReader>(
        &self,
        state: &mut TransactionalState<'_, S>,
        tx_context: Arc<TransactionContext>,
        remaining_gas: &mut GasCounter,
    ) -> TransactionExecutionResult<ValidateExecuteCallInfo> {
        let validate_call_info: Option<CallInfo>;
        let execute_call_info: Option<CallInfo>;
        if matches!(&self.tx, Transaction::DeployAccount(_)) {
            // Handle `DeployAccount` transactions separately, due to different order of things.
            // Also, the execution context required for the `DeployAccount` execute phase is
            // validation context.
            let mut execution_context = EntryPointExecutionContext::new_validate(
                tx_context.clone(),
                self.execution_flags.charge_fee,
                // TODO(Dori): Reduce code dup (the gas usage limit is computed in run_execute).
                // We initialize the revert gas tracker here for completeness - the value will not
                // be used, as this tx is non-revertible.
                SierraGasRevertTracker::new(GasAmount(
                    remaining_gas
                        .limit_usage(tx_context.sierra_gas_limit(&ExecutionMode::Validate)),
                )),
            );
            execute_call_info = self.run_execute(state, &mut execution_context, remaining_gas)?;
            validate_call_info = self.validate_tx(state, tx_context.clone(), remaining_gas)?;
        } else {
            validate_call_info = self.validate_tx(state, tx_context.clone(), remaining_gas)?;
            let mut execution_context = EntryPointExecutionContext::new_invoke(
                tx_context.clone(),
                self.execution_flags.charge_fee,
                // TODO(Dori): Reduce code dup (the gas usage limit is computed in run_execute).
                // We initialize the revert gas tracker here for completeness - the value will not
                // be used, as this tx is non-revertible.
                SierraGasRevertTracker::new(GasAmount(
                    remaining_gas.limit_usage(tx_context.sierra_gas_limit(&ExecutionMode::Execute)),
                )),
            );
            execute_call_info = self.run_execute(state, &mut execution_context, remaining_gas)?;
        }

        let tx_receipt = TransactionReceipt::from_account_tx(
            self,
            &tx_context,
            &state.to_state_diff()?,
            CallInfo::summarize_many(
                validate_call_info.iter().chain(execute_call_info.iter()),
                &tx_context.block_context.versioned_constants,
            ),
            0,
            GasAmount(0),
        );

        let post_execution_report = PostExecutionReport::new(
            state,
            &tx_context,
            &tx_receipt,
            self.execution_flags.charge_fee,
        )?;
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
        remaining_gas: &mut GasCounter,
    ) -> TransactionExecutionResult<ValidateExecuteCallInfo> {
        // Run the validation, and if execution later fails, only keep the validation diff.
        let validate_call_info = self.validate_tx(state, tx_context.clone(), remaining_gas)?;

        let mut execution_context = EntryPointExecutionContext::new_invoke(
            tx_context.clone(),
            self.execution_flags.charge_fee,
            // TODO(Dori): Reduce code dup (the gas usage limit is computed in run_execute).
            SierraGasRevertTracker::new(GasAmount(
                remaining_gas.limit_usage(tx_context.sierra_gas_limit(&ExecutionMode::Execute)),
            )),
        );
        let n_allotted_execution_steps = execution_context.subtract_validation_and_overhead_steps(
            &validate_call_info,
            &self.tx_type(),
            self.calldata_length(),
        );

        // Save the state changes resulting from running `validate_tx`, to be used later for
        // resource and fee calculation.
        let validate_state_cache = state.borrow_updated_state_cache()?.clone();

        // Create copies of state and validate_resources for the execution.
        // Both will be rolled back if the execution is reverted or committed upon success.
        let mut execution_state = TransactionalState::create_transactional(state);

        let execution_result =
            self.run_execute(&mut execution_state, &mut execution_context, remaining_gas);

        // Pre-compute cost in case of revert.
        let execution_steps_consumed =
            n_allotted_execution_steps - execution_context.n_remaining_steps();
        // Get the receipt only in case of revert.
        let get_revert_receipt = || {
            TransactionReceipt::from_account_tx(
                self,
                &tx_context,
                &validate_state_cache.to_state_diff(),
                CallInfo::summarize_many(
                    validate_call_info.iter(),
                    &tx_context.block_context.versioned_constants,
                ),
                execution_steps_consumed,
                execution_context.sierra_gas_revert_tracker.get_gas_consumed(),
            )
        };

        match execution_result {
            Ok(execute_call_info) => {
                // When execution succeeded, calculate the actual required fee before committing the
                // transactional state. If max_fee is insufficient, revert the `run_execute` part.
                let tx_receipt = TransactionReceipt::from_account_tx(
                    self,
                    &tx_context,
                    &StateCache::squash_state_diff(
                        vec![
                            &validate_state_cache,
                            &execution_state.borrow_updated_state_cache()?.clone(),
                        ],
                        tx_context.block_context.versioned_constants.comprehensive_state_diff,
                    ),
                    CallInfo::summarize_many(
                        validate_call_info.iter().chain(execute_call_info.iter()),
                        &tx_context.block_context.versioned_constants,
                    ),
                    0,
                    GasAmount(0),
                );
                // Post-execution checks.
                let post_execution_report = PostExecutionReport::new(
                    &mut execution_state,
                    &tx_context,
                    &tx_receipt,
                    self.execution_flags.charge_fee,
                )?;
                match post_execution_report.error() {
                    Some(post_execution_error) => {
                        // Post-execution check failed. Revert the execution, compute the final fee
                        // to charge and recompute resources used (to be consistent with other
                        // revert case, compute resources by adding consumed execution steps to
                        // validation resources).
                        execution_state.abort();
                        let tx_receipt = TransactionReceipt {
                            fee: post_execution_report.recommended_fee(),
                            ..get_revert_receipt()
                        };
                        Ok(ValidateExecuteCallInfo::new_reverted(
                            validate_call_info,
                            post_execution_error.into(),
                            tx_receipt,
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
                let revert_receipt = get_revert_receipt();
                // Error during execution. Revert, even if the error is sequencer-related.
                execution_state.abort();
                let post_execution_report = PostExecutionReport::new(
                    state,
                    &tx_context,
                    &revert_receipt,
                    self.execution_flags.charge_fee,
                )?;
                Ok(ValidateExecuteCallInfo::new_reverted(
                    validate_call_info,
                    gen_tx_execution_error_trace(&execution_error).into(),
                    TransactionReceipt {
                        fee: post_execution_report.recommended_fee(),
                        ..revert_receipt
                    },
                ))
            }
        }
    }

    /// Returns 0 on non-declare transactions; for declare transactions, returns the class code
    /// size.
    pub(crate) fn declare_code_size(&self) -> usize {
        if let Transaction::Declare(tx) = &self.tx { tx.class_info.code_size() } else { 0 }
    }

    fn is_non_revertible(&self, tx_info: &TransactionInfo) -> bool {
        // Reverting a Declare or Deploy transaction is not currently supported in the OS.
        match &self.tx {
            Transaction::Declare(_) => true,
            Transaction::DeployAccount(_) => true,
            Transaction::Invoke(_) => {
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
        remaining_gas: &mut GasCounter,
        tx_context: Arc<TransactionContext>,
    ) -> TransactionExecutionResult<ValidateExecuteCallInfo> {
        if self.is_non_revertible(&tx_context.tx_info) {
            return self.run_non_revertible(state, tx_context, remaining_gas);
        }

        self.run_revertible(state, tx_context, remaining_gas)
    }
}

impl<U: UpdatableState> ExecutableTransaction<U> for AccountTransaction {
    fn execute_raw(
        &self,
        state: &mut TransactionalState<'_, U>,
        block_context: &BlockContext,
        concurrency_mode: bool,
    ) -> TransactionExecutionResult<TransactionExecutionInfo> {
        let tx_context = Arc::new(block_context.to_tx_context(self));
        self.verify_tx_version(tx_context.tx_info.version())?;

        // Do not run validate or perform any account-related actions for declare transactions that
        // meet the following conditions.
        // This flow is used for the sequencer to bootstrap a new system.
        if let Transaction::Declare(tx) = &self.tx {
            if tx.is_bootstrap_declare(self.execution_flags.charge_fee) {
                let mut context = EntryPointExecutionContext::new_invoke(
                    tx_context.clone(),
                    self.execution_flags.charge_fee,
                    SierraGasRevertTracker::new(GasAmount::default()),
                );
                let mut remaining_gas = 0;
                let res = tx.run_execute(state, &mut context, &mut remaining_gas)?;
                assert!(res.is_none(), "Declare execute should not result in a CallInfo.");

                return Ok(TransactionExecutionInfo::default());
            }
        }

        // Nonce and fee check should be done before running user code.
        self.perform_pre_validation_stage(state, &tx_context)?;

        // Run validation and execution.
        let initial_gas = tx_context.initial_sierra_gas();
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
        } = self.run_or_revert(state, &mut GasCounter::new(initial_gas), tx_context.clone())?;
        let fee_transfer_call_info = Self::handle_fee(
            state,
            tx_context,
            final_fee,
            self.execution_flags.charge_fee,
            concurrency_mode,
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
        self.tx.create_tx_info(self.execution_flags.only_query)
    }
}

/// Represents a bundle of validate-execute stage execution effects.
struct ValidateExecuteCallInfo {
    validate_call_info: Option<CallInfo>,
    execute_call_info: Option<CallInfo>,
    revert_error: Option<RevertError>,
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
        revert_error: RevertError,
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
        tx_context: Arc<TransactionContext>,
        remaining_gas: &mut GasCounter,
    ) -> TransactionExecutionResult<Option<CallInfo>> {
        if !self.execution_flags.validate {
            return Ok(None);
        }
        let remaining_validation_gas = &mut remaining_gas.limit_usage(
            tx_context.block_context.versioned_constants.os_constants.validate_max_sierra_gas,
        );
        let limit_steps_by_resources = self.execution_flags.charge_fee;
        let mut context = EntryPointExecutionContext::new_validate(
            tx_context,
            limit_steps_by_resources,
            SierraGasRevertTracker::new(GasAmount(*remaining_validation_gas)),
        );
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
            initial_gas: *remaining_validation_gas,
        };

        // Note that we allow a revert here and we handle it bellow to get a better error message.
        let validate_call_info = validate_call
            .execute(state, &mut context, remaining_validation_gas)
            .map_err(|error| TransactionExecutionError::ValidateTransactionError {
                error,
                class_hash,
                storage_address,
                selector: validate_selector,
            })?;

        // Validate return data.
        let compiled_class = state.get_compiled_class(class_hash)?;
        if is_cairo1(&compiled_class) {
            // The account contract class is a Cairo 1.0 contract; the `validate` entry point should
            // return `VALID`.
            let expected_retdata = retdata![*constants::VALIDATE_RETDATA];

            if validate_call_info.execution.failed {
                return Err(TransactionExecutionError::PanicInValidate {
                    panic_reason: extract_trailing_cairo1_revert_trace(
                        &validate_call_info,
                        Cairo1RevertHeader::Validation,
                    ),
                });
            }

            if validate_call_info.execution.retdata != expected_retdata {
                return Err(TransactionExecutionError::InvalidValidateReturnData {
                    actual: validate_call_info.execution.retdata,
                });
            }
        }
        remaining_gas.subtract_used_gas(&validate_call_info);
        Ok(Some(validate_call_info))
    }
}

pub fn is_cairo1(compiled_class: &RunnableCompiledClass) -> bool {
    match compiled_class {
        RunnableCompiledClass::V0(_) => false,
        RunnableCompiledClass::V1(_) => true,
        #[cfg(feature = "cairo_native")]
        RunnableCompiledClass::V1Native(_) => true,
    }
}
