use std::cell::RefCell;
use std::cmp::min;
use std::collections::HashMap;
use std::sync::Arc;

use cairo_vm::vm::runners::cairo_runner::{ResourceTracker, RunResources};
use num_traits::{Inv, Zero};
use serde::Serialize;
use starknet_api::abi::abi_utils::selector_from_name;
use starknet_api::abi::constants::CONSTRUCTOR_ENTRY_POINT_NAME;
use starknet_api::contract_class::EntryPointType;
use starknet_api::core::{ClassHash, ContractAddress, EntryPointSelector};
use starknet_api::executable_transaction::TransactionType;
use starknet_api::execution_resources::GasAmount;
use starknet_api::state::StorageKey;
use starknet_api::transaction::fields::{
    AllResourceBounds,
    Calldata,
    ResourceBounds,
    ValidResourceBounds,
    HIGH_GAS_AMOUNT,
};
use starknet_api::transaction::TransactionVersion;
use starknet_types_core::felt::Felt;

use crate::blockifier_versioned_constants::{GasCosts, VersionedConstants};
use crate::context::{BlockContext, TransactionContext};
use crate::execution::call_info::CallInfo;
use crate::execution::common_hints::ExecutionMode;
use crate::execution::contract_class::{RunnableCompiledClass, TrackedResource};
use crate::execution::errors::{
    ConstructorEntryPointExecutionError,
    EntryPointExecutionError,
    PreExecutionError,
};
use crate::execution::execution_utils::execute_entry_point_call_wrapper;
use crate::execution::stack_trace::{extract_trailing_cairo1_revert_trace, Cairo1RevertHeader};
use crate::state::cached_state::CachedState;
use crate::state::state_api::{State, StateReader, StateResult};
use crate::transaction::objects::{HasRelatedFeeType, TransactionInfo};
use crate::utils::usize_from_u64;

#[cfg(test)]
#[path = "entry_point_test.rs"]
pub mod test;

pub const FAULTY_CLASS_HASH: &str =
    "0x1A7820094FEAF82D53F53F214B81292D717E7BB9A92BB2488092CD306F3993F";

pub type EntryPointExecutionResult<T> = Result<T, EntryPointExecutionError>;
pub type ConstructorEntryPointExecutionResult<T> = Result<T, ConstructorEntryPointExecutionError>;

/// Holds the the information required to revert the execution of an entry point.
#[derive(Debug)]
pub struct EntryPointRevertInfo {
    // The contract address that the revert info applies to.
    pub contract_address: ContractAddress,
    /// The original class hash of the contract that was called.
    pub original_class_hash: ClassHash,
    /// The original storage values.
    pub original_values: HashMap<StorageKey, Felt>,
    // The number of emitted events before the call.
    n_emitted_events: usize,
    // The number of sent messages to L1 before the call.
    n_sent_messages_to_l1: usize,
}
impl EntryPointRevertInfo {
    pub fn new(
        contract_address: ContractAddress,
        original_class_hash: ClassHash,
        n_emitted_events: usize,
        n_sent_messages_to_l1: usize,
    ) -> Self {
        Self {
            contract_address,
            original_class_hash,
            original_values: HashMap::new(),
            n_emitted_events,
            n_sent_messages_to_l1,
        }
    }
}

/// The ExecutionRevertInfo stores a vector of entry point revert infos.
/// We don't merge infos related same contract as doing it on every nesting level would
/// result in O(N^2) complexity.
#[derive(Default, Debug)]
pub struct ExecutionRevertInfo(pub Vec<EntryPointRevertInfo>);

/// Represents a the type of the call (used for debugging).
#[cfg_attr(feature = "transaction_serde", derive(serde::Deserialize))]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize)]
pub enum CallType {
    #[default]
    Call = 0,
    Delegate = 1,
}

pub struct EntryPointTypeAndSelector {
    pub entry_point_type: EntryPointType,
    pub entry_point_selector: EntryPointSelector,
}

impl EntryPointTypeAndSelector {
    pub fn verify_constructor(&self) -> Result<(), PreExecutionError> {
        if self.entry_point_type == EntryPointType::Constructor
            && self.entry_point_selector != selector_from_name(CONSTRUCTOR_ENTRY_POINT_NAME)
        {
            Err(PreExecutionError::InvalidConstructorEntryPointName)
        } else {
            Ok(())
        }
    }
}

/// Represents a call to an entry point of a Starknet contract.
#[cfg_attr(feature = "transaction_serde", derive(serde::Deserialize))]
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct CallEntryPointVariant<TClassHash> {
    /// The class hash of the entry point.
    /// The type is `ClassHash` in the case of [ExecutableCallEntryPoint] and `Option<ClassHash>`
    /// in the case of [CallEntryPoint].
    ///
    /// The class hash is not given if it can be deduced from the storage address.
    /// It is resolved prior to entry point's execution.
    pub class_hash: TClassHash,
    // Optional, since there is no address to the code implementation in a library call.
    // and for outermost calls (triggered by the transaction itself).
    // TODO(AlonH): BACKWARD-COMPATIBILITY.
    pub code_address: Option<ContractAddress>,
    pub entry_point_type: EntryPointType,
    pub entry_point_selector: EntryPointSelector,
    pub calldata: Calldata,
    pub storage_address: ContractAddress,
    pub caller_address: ContractAddress,
    pub call_type: CallType,
    // We can assume that the initial gas is less than 2^64.
    pub initial_gas: u64,
}

pub type CallEntryPoint = CallEntryPointVariant<Option<ClassHash>>;
pub type ExecutableCallEntryPoint = CallEntryPointVariant<ClassHash>;

impl From<ExecutableCallEntryPoint> for CallEntryPoint {
    fn from(call: ExecutableCallEntryPoint) -> Self {
        Self {
            class_hash: Some(call.class_hash),
            code_address: call.code_address,
            entry_point_type: call.entry_point_type,
            entry_point_selector: call.entry_point_selector,
            calldata: call.calldata,
            storage_address: call.storage_address,
            caller_address: call.caller_address,
            call_type: call.call_type,
            initial_gas: call.initial_gas,
        }
    }
}

impl CallEntryPoint {
    pub fn execute(
        mut self,
        state: &mut dyn State,
        context: &mut EntryPointExecutionContext,
        remaining_gas: &mut u64,
    ) -> EntryPointExecutionResult<CallInfo> {
        let tx_context = &context.tx_context;
        let mut decrement_when_dropped = RecursionDepthGuard::new(
            context.current_recursion_depth.clone(),
            context.versioned_constants().max_recursion_depth,
        );
        decrement_when_dropped.try_increment_and_check_depth()?;

        // Validate contract is deployed.
        let storage_class_hash = state.get_class_hash_at(self.storage_address)?;
        if storage_class_hash == ClassHash::default() {
            return Err(PreExecutionError::UninitializedStorageAddress(self.storage_address).into());
        }

        let class_hash = match self.class_hash {
            Some(class_hash) => class_hash,
            None => storage_class_hash, // If not given, take the storage contract class hash.
        };
        // Hack to prevent version 0 attack on argent accounts.
        if tx_context.tx_info.version() == TransactionVersion::ZERO
            && class_hash
                == ClassHash(
                    Felt::from_hex(FAULTY_CLASS_HASH).expect("A class hash must be a felt."),
                )
        {
            return Err(PreExecutionError::FraudAttempt.into());
        }
        // Add class hash to the call, that will appear in the output (call info).
        self.class_hash = Some(class_hash);
        let compiled_class = state.get_compiled_class(class_hash)?;

        context.revert_infos.0.push(EntryPointRevertInfo::new(
            self.storage_address,
            storage_class_hash,
            context.n_emitted_events,
            context.n_sent_messages_to_l1,
        ));

        // This is the last operation of this function.
        execute_entry_point_call_wrapper(
            self.into_executable(class_hash),
            compiled_class,
            state,
            context,
            remaining_gas,
        )
    }

    /// Similar to `execute`, but returns an error if the outer call is reverted.
    pub fn non_reverting_execute(
        self,
        state: &mut dyn State,
        context: &mut EntryPointExecutionContext,
        remaining_gas: &mut u64,
    ) -> EntryPointExecutionResult<CallInfo> {
        let execution_result = self.execute(state, context, remaining_gas);
        if let Ok(call_info) = &execution_result {
            // Update revert gas tracking (for completeness - value will not be used unless the tx
            // is reverted).
            context.sierra_gas_revert_tracker.update_with_next_remaining_gas(
                call_info.tracked_resource,
                GasAmount(*remaining_gas),
            );
            // If the execution of the outer call failed, revert the transction.
            if call_info.execution.failed {
                return Err(EntryPointExecutionError::ExecutionFailed {
                    error_trace: extract_trailing_cairo1_revert_trace(
                        call_info,
                        Cairo1RevertHeader::Execution,
                    ),
                });
            }
        }

        execution_result
    }

    pub fn into_executable(self, class_hash: ClassHash) -> ExecutableCallEntryPoint {
        ExecutableCallEntryPoint {
            class_hash,
            code_address: self.code_address,
            entry_point_type: self.entry_point_type,
            entry_point_selector: self.entry_point_selector,
            calldata: self.calldata,
            storage_address: self.storage_address,
            caller_address: self.caller_address,
            call_type: self.call_type,
            initial_gas: self.initial_gas,
        }
    }
}

impl ExecutableCallEntryPoint {
    pub fn type_and_selector(&self) -> EntryPointTypeAndSelector {
        EntryPointTypeAndSelector {
            entry_point_type: self.entry_point_type,
            entry_point_selector: self.entry_point_selector,
        }
    }
}

pub struct ConstructorContext {
    pub class_hash: ClassHash,
    // Only relevant in deploy syscall.
    pub code_address: Option<ContractAddress>,
    pub storage_address: ContractAddress,
    pub caller_address: ContractAddress,
}

#[derive(Debug)]
pub struct SierraGasRevertTracker {
    initial_remaining_gas: GasAmount,
    last_seen_remaining_gas: GasAmount,
}

impl SierraGasRevertTracker {
    pub fn new(initial_remaining_gas: GasAmount) -> Self {
        Self { initial_remaining_gas, last_seen_remaining_gas: initial_remaining_gas }
    }

    /// Updates the last seen remaining gas, if we are in gas-tracking mode.
    pub fn update_with_next_remaining_gas(
        &mut self,
        tracked_resource: TrackedResource,
        next_remaining_gas: GasAmount,
    ) {
        if tracked_resource == TrackedResource::SierraGas {
            self.last_seen_remaining_gas = next_remaining_gas;
        }
    }

    pub fn get_gas_consumed(&self) -> GasAmount {
        self.initial_remaining_gas.checked_sub(self.last_seen_remaining_gas).unwrap_or_else(|| {
            panic!(
                "The consumed gas must be non-negative. Initial gas: {}, last seen gas: {}.",
                self.initial_remaining_gas, self.last_seen_remaining_gas
            )
        })
    }
}

#[derive(Debug)]
pub struct EntryPointExecutionContext {
    // We use `Arc` to avoid the clone of this potentially large object, as inner calls
    // are created during execution.
    pub tx_context: Arc<TransactionContext>,
    // VM execution limits.
    pub vm_run_resources: RunResources,
    /// Used for tracking events order during the current execution.
    pub n_emitted_events: usize,
    /// Used for tracking L2-to-L1 messages order during the current execution.
    pub n_sent_messages_to_l1: usize,
    // Managed by dedicated guard object.
    current_recursion_depth: Arc<RefCell<usize>>,

    // The execution mode affects the behavior of the hint processor.
    pub execution_mode: ExecutionMode,
    // The call stack of tracked resources from the first entry point to the current.
    pub tracked_resource_stack: Vec<TrackedResource>,

    // Information for reverting the state (inludes the revert info of the callers).
    pub revert_infos: ExecutionRevertInfo,

    // Used to support charging for gas consumed in blockifier revert flow.
    pub sierra_gas_revert_tracker: SierraGasRevertTracker,
}

impl EntryPointExecutionContext {
    pub fn new(
        tx_context: Arc<TransactionContext>,
        mode: ExecutionMode,
        limit_steps_by_resources: bool,
        sierra_gas_revert_tracker: SierraGasRevertTracker,
    ) -> Self {
        let max_steps = Self::max_steps(&tx_context, &mode, limit_steps_by_resources);
        Self {
            vm_run_resources: RunResources::new(max_steps),
            n_emitted_events: 0,
            n_sent_messages_to_l1: 0,
            tx_context: tx_context.clone(),
            current_recursion_depth: Default::default(),
            execution_mode: mode,
            tracked_resource_stack: vec![],
            revert_infos: ExecutionRevertInfo(vec![]),
            sierra_gas_revert_tracker,
        }
    }

    pub fn new_validate(
        tx_context: Arc<TransactionContext>,
        limit_steps_by_resources: bool,
        sierra_gas_revert_tracker: SierraGasRevertTracker,
    ) -> Self {
        Self::new(
            tx_context,
            ExecutionMode::Validate,
            limit_steps_by_resources,
            sierra_gas_revert_tracker,
        )
    }

    pub fn new_invoke(
        tx_context: Arc<TransactionContext>,
        limit_steps_by_resources: bool,
        sierra_gas_revert_tracker: SierraGasRevertTracker,
    ) -> Self {
        Self::new(
            tx_context,
            ExecutionMode::Execute,
            limit_steps_by_resources,
            sierra_gas_revert_tracker,
        )
    }

    /// Returns the maximum number of cairo steps allowed, given the max fee, gas price and the
    /// execution mode.
    /// If fee is disabled, returns the global maximum.
    /// The bound computation is saturating (no panic on overflow).
    fn max_steps(
        tx_context: &TransactionContext,
        mode: &ExecutionMode,
        limit_steps_by_resources: bool,
    ) -> usize {
        let TransactionContext { block_context, tx_info } = tx_context;
        let BlockContext { block_info, versioned_constants, .. } = block_context.as_ref();
        let block_upper_bound = match mode {
            ExecutionMode::Validate => versioned_constants.validate_max_n_steps,
            ExecutionMode::Execute => versioned_constants.invoke_tx_max_n_steps,
        }
        .try_into()
        .unwrap_or_else(|error| {
            log::warn!("Failed to convert global step limit to to usize: {error}.");
            usize::MAX
        });

        if !limit_steps_by_resources {
            return block_upper_bound;
        }

        // Deprecated transactions derive the step limit from the `max_fee`, by computing the L1 gas
        // limit induced by the max fee and translating into cairo steps.
        // New transactions with only L1 bounds use the L1 resource bounds directly.
        // New transactions with L2 bounds use the L2 bounds directly.
        let l1_gas_per_step = versioned_constants.vm_resource_fee_cost().n_steps;
        let l2_gas_per_step = versioned_constants.os_constants.gas_costs.base.step_gas_cost;

        let tx_upper_bound_u64 = match tx_info {
            // Fee is a larger uint type than GasAmount, so we need to saturate the division.
            // This is just a computation of an upper bound, so it's safe to saturate.
            TransactionInfo::Deprecated(context) => {
                if l1_gas_per_step.is_zero() {
                    u64::MAX
                } else {
                    let induced_l1_gas_limit = context
                        .max_fee
                        .saturating_div(block_info.gas_prices.l1_gas_price(&tx_info.fee_type()));
                    (l1_gas_per_step.inv() * induced_l1_gas_limit.0).to_integer()
                }
            }
            TransactionInfo::Current(context) => match context.resource_bounds {
                ValidResourceBounds::L1Gas(ResourceBounds { max_amount, .. }) => {
                    if l1_gas_per_step.is_zero() {
                        u64::MAX
                    } else {
                        (l1_gas_per_step.inv() * max_amount.0).to_integer()
                    }
                }
                ValidResourceBounds::AllResources(AllResourceBounds {
                    l2_gas: ResourceBounds { max_amount, .. },
                    ..
                }) => {
                    if l2_gas_per_step.is_zero() {
                        u64::MAX
                    } else {
                        max_amount.0.saturating_div(l2_gas_per_step)
                    }
                }
            },
        };

        // Use saturating upper bound to avoid overflow. This is safe because the upper bound is
        // bounded above by the block's limit, which is a usize.
        let tx_upper_bound = usize_from_u64(tx_upper_bound_u64).unwrap_or_else(|_| {
            log::warn!("Failed to convert u64 to usize: {tx_upper_bound_u64}.");
            usize::MAX
        });
        min(tx_upper_bound, block_upper_bound)
    }

    /// Returns the available steps in run resources.
    pub fn n_remaining_steps(&self) -> usize {
        self.vm_run_resources.get_n_steps().expect("The number of steps must be initialized.")
    }

    /// Subtracts the given number of steps from the currently available run resources.
    /// Used for limiting the number of steps available during the execution stage, to leave enough
    /// steps available for the fee transfer stage.
    /// Returns the remaining number of steps.
    pub fn subtract_steps(&mut self, steps_to_subtract: usize) -> usize {
        // If remaining steps is less than the number of steps to subtract, attempting to subtrace
        // would cause underflow error.
        // Logically, we update remaining steps to `max(0, remaining_steps - steps_to_subtract)`.
        let remaining_steps = self.n_remaining_steps();
        let new_remaining_steps = remaining_steps.saturating_sub(steps_to_subtract);
        self.vm_run_resources = RunResources::new(new_remaining_steps);
        self.n_remaining_steps()
    }

    /// From the total amount of steps available for execution, deduct the steps consumed during
    /// validation and the overhead steps required, among the rest, for fee transfer.
    /// Returns the remaining steps (after the subtraction).
    pub fn subtract_validation_and_overhead_steps(
        &mut self,
        validate_call_info: &Option<CallInfo>,
        tx_type: &TransactionType,
        calldata_length: usize,
    ) -> usize {
        let validate_steps = validate_call_info
            .as_ref()
            .map(|call_info| call_info.resources.n_steps)
            .unwrap_or_default();

        let overhead_steps =
            self.versioned_constants().os_resources_for_tx_type(tx_type, calldata_length).n_steps;
        self.subtract_steps(validate_steps + overhead_steps)
    }

    /// Calls update_with_next_remaining_gas if the tracked resource is sierra gas.
    pub fn update_revert_gas_with_next_remaining_gas(&mut self, next_remaining_gas: GasAmount) {
        self.sierra_gas_revert_tracker.update_with_next_remaining_gas(
            *self
                .tracked_resource_stack
                .last()
                .expect("Tracked resource stack should not be empty at this point."),
            next_remaining_gas,
        );
    }

    pub fn versioned_constants(&self) -> &VersionedConstants {
        &self.tx_context.block_context.versioned_constants
    }

    pub fn gas_costs(&self) -> &GasCosts {
        &self.versioned_constants().os_constants.gas_costs
    }

    /// Reverts the state back to the way it was when self.revert_infos.0['revert_idx'] was created.
    pub fn revert(&mut self, revert_idx: usize, state: &mut dyn State) -> StateResult<()> {
        for contract_revert_info in self.revert_infos.0.drain(revert_idx..).rev() {
            for (key, value) in contract_revert_info.original_values.iter() {
                state.set_storage_at(contract_revert_info.contract_address, *key, *value)?;
            }
            state.set_class_hash_at(
                contract_revert_info.contract_address,
                contract_revert_info.original_class_hash,
            )?;

            self.n_emitted_events = contract_revert_info.n_emitted_events;
            self.n_sent_messages_to_l1 = contract_revert_info.n_sent_messages_to_l1;
        }

        Ok(())
    }
}

pub fn execute_constructor_entry_point(
    state: &mut dyn State,
    context: &mut EntryPointExecutionContext,
    ctor_context: ConstructorContext,
    calldata: Calldata,
    remaining_gas: &mut u64,
) -> ConstructorEntryPointExecutionResult<CallInfo> {
    // Ensure the class is declared (by reading it).
    let compiled_class = state.get_compiled_class(ctor_context.class_hash).map_err(|error| {
        ConstructorEntryPointExecutionError::new(error.into(), &ctor_context, None)
    })?;
    let Some(constructor_selector) = compiled_class.constructor_selector() else {
        // Contract has no constructor.
        return handle_empty_constructor(
            compiled_class,
            context,
            &ctor_context,
            calldata,
            *remaining_gas,
        )
        .map_err(|error| ConstructorEntryPointExecutionError::new(error, &ctor_context, None));
    };

    let constructor_call = CallEntryPoint {
        class_hash: None,
        code_address: ctor_context.code_address,
        entry_point_type: EntryPointType::Constructor,
        entry_point_selector: constructor_selector,
        calldata,
        storage_address: ctor_context.storage_address,
        caller_address: ctor_context.caller_address,
        call_type: CallType::Call,
        initial_gas: *remaining_gas,
    };

    constructor_call.non_reverting_execute(state, context, remaining_gas).map_err(|error| {
        ConstructorEntryPointExecutionError::new(error, &ctor_context, Some(constructor_selector))
    })
}

// Calls the specified external entry point on the contract at the given address.
// Intended for view-only entry points; any attempted state changes will be discarded.
pub fn call_view_entry_point(
    state_reader: impl StateReader,
    block_context: Arc<BlockContext>,
    storage_address: ContractAddress,
    entry_point_name: &str,
    calldata: Calldata,
) -> EntryPointExecutionResult<CallInfo> {
    let mut initial_gas = GasAmount(HIGH_GAS_AMOUNT);

    let execute_call = CallEntryPoint {
        entry_point_type: EntryPointType::External,
        entry_point_selector: selector_from_name(entry_point_name),
        calldata,
        class_hash: None,
        code_address: None,
        storage_address,
        caller_address: ContractAddress::default(),
        call_type: CallType::Call,
        initial_gas: initial_gas.0,
    };

    // Create a dummy transaction info, since we are not in a context of a real transaction.
    let tx_context =
        Arc::new(TransactionContext { block_context, tx_info: TransactionInfo::default() });

    let limit_steps = false;
    let mut context = EntryPointExecutionContext::new(
        tx_context,
        ExecutionMode::Execute,
        limit_steps,
        SierraGasRevertTracker::new(initial_gas),
    );

    let mut state = CachedState::new(state_reader); // Changes to it are discarded.
    execute_call.non_reverting_execute(&mut state, &mut context, &mut initial_gas.0)
}

pub fn handle_empty_constructor(
    compiled_class: RunnableCompiledClass,
    context: &mut EntryPointExecutionContext,
    ctor_context: &ConstructorContext,
    calldata: Calldata,
    remaining_gas: u64,
) -> EntryPointExecutionResult<CallInfo> {
    // Validate no calldata.
    if !calldata.0.is_empty() {
        return Err(EntryPointExecutionError::InvalidExecutionInput {
            input_descriptor: "constructor_calldata".to_string(),
            info: "Cannot pass calldata to a contract with no constructor.".to_string(),
        });
    }

    let current_tracked_resource = compiled_class.get_current_tracked_resource(context);
    let initial_gas = if current_tracked_resource == TrackedResource::CairoSteps {
        // Override the initial gas with a high value to be consistent with the behavior for the
        // rest of the CairoSteps mode calls.
        context.versioned_constants().infinite_gas_for_vm_mode()
    } else {
        remaining_gas
    };
    let empty_constructor_call_info = CallInfo {
        call: CallEntryPoint {
            class_hash: Some(ctor_context.class_hash),
            code_address: ctor_context.code_address,
            entry_point_type: EntryPointType::Constructor,
            entry_point_selector: selector_from_name(CONSTRUCTOR_ENTRY_POINT_NAME),
            calldata: Calldata::default(),
            storage_address: ctor_context.storage_address,
            caller_address: ctor_context.caller_address,
            call_type: CallType::Call,
            initial_gas,
        },
        tracked_resource: current_tracked_resource,
        ..Default::default()
    };

    Ok(empty_constructor_call_info)
}

// Ensure that the recursion depth does not exceed the maximum allowed depth.
struct RecursionDepthGuard {
    current_depth: Arc<RefCell<usize>>,
    max_depth: usize,
}

impl RecursionDepthGuard {
    fn new(current_depth: Arc<RefCell<usize>>, max_depth: usize) -> Self {
        Self { current_depth, max_depth }
    }

    // Tries to increment the current recursion depth and returns an error if the maximum depth
    // would be exceeded.
    fn try_increment_and_check_depth(&mut self) -> EntryPointExecutionResult<()> {
        *self.current_depth.borrow_mut() += 1;
        if *self.current_depth.borrow() > self.max_depth {
            return Err(EntryPointExecutionError::RecursionDepthExceeded);
        }
        Ok(())
    }
}

// Implementing the Drop trait to decrement the recursion depth when the guard goes out of scope.
impl Drop for RecursionDepthGuard {
    fn drop(&mut self) {
        *self.current_depth.borrow_mut() -= 1;
    }
}
