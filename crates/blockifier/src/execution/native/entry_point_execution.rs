use cairo_native::execution_result::ContractExecutionResult;
use cairo_native::utils::BuiltinCosts;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use starknet_api::execution_resources::GasAmount;

use crate::execution::call_info::{CallExecution, CallInfo, ChargedResources, Retdata};
use crate::execution::contract_class::TrackedResource;
use crate::execution::entry_point::{
    CallEntryPoint,
    EntryPointExecutionContext,
    EntryPointExecutionResult,
};
use crate::execution::errors::{EntryPointExecutionError, PostExecutionError};
use crate::execution::native::contract_class::NativeContractClassV1;
use crate::execution::native::syscall_handler::NativeSyscallHandler;
use crate::state::state_api::State;

// todo(rodrigo): add an `entry point not found` test for Native
pub fn execute_entry_point_call(
    call: CallEntryPoint,
    contract_class: NativeContractClassV1,
    state: &mut dyn State,
    context: &mut EntryPointExecutionContext,
) -> EntryPointExecutionResult<CallInfo> {
    let entry_point = contract_class.get_entry_point(&call)?;

    let mut syscall_handler: NativeSyscallHandler<'_> =
        NativeSyscallHandler::new(call, state, context);

    let gas_costs = &syscall_handler.base.context.versioned_constants().os_constants.gas_costs;
    let builtin_costs = BuiltinCosts {
        // todo(rodrigo): Unsure of what value `const` means, but 1 is the right value
        r#const: 1,
        pedersen: gas_costs.pedersen_gas_cost,
        bitwise: gas_costs.bitwise_builtin_gas_cost,
        ecop: gas_costs.ecop_gas_cost,
        poseidon: gas_costs.poseidon_gas_cost,
        add_mod: gas_costs.add_mod_gas_cost,
        mul_mod: gas_costs.mul_mod_gas_cost,
    };

    // Fund the initial budget since the native executor charges it before the run.
    // TODO(Yoni): revert once the VM is aligned with this.
    let gas = syscall_handler.base.call.initial_gas + gas_costs.entry_point_initial_budget;
    let execution_result = contract_class.executor.run(
        entry_point.selector.0,
        &syscall_handler.base.call.calldata.0.clone(),
        Some(gas),
        Some(builtin_costs),
        &mut syscall_handler,
    );
    syscall_handler.finalize();

    let call_result = execution_result.map_err(EntryPointExecutionError::NativeUnexpectedError)?;

    if let Some(error) = syscall_handler.unrecoverable_error {
        return Err(EntryPointExecutionError::NativeUnrecoverableError(Box::new(error)));
    }

    create_callinfo(call_result, syscall_handler)
}

fn create_callinfo(
    call_result: ContractExecutionResult,
    syscall_handler: NativeSyscallHandler<'_>,
) -> Result<CallInfo, EntryPointExecutionError> {
    let mut remaining_gas = call_result.remaining_gas;

    if remaining_gas > syscall_handler.base.call.initial_gas {
        if remaining_gas - syscall_handler.base.call.initial_gas
            <= syscall_handler.base.context.gas_costs().entry_point_initial_budget
        {
            // Revert the refund.
            // TODO(Yoni): temporary hack - this is probably a bug. Investigate and fix native.
            remaining_gas = syscall_handler.base.call.initial_gas;
        } else {
            return Err(PostExecutionError::MalformedReturnData {
                error_message: format!(
                    "Unexpected remaining gas. Used gas is greater than initial gas: {} > {}",
                    remaining_gas, syscall_handler.base.call.initial_gas
                ),
            }
            .into());
        }
    }

    let gas_consumed = syscall_handler.base.call.initial_gas - remaining_gas;

    let charged_resources_without_inner_calls = ChargedResources {
        vm_resources: ExecutionResources::default(),
        // TODO(tzahi): Replace with a computed value.
        gas_for_fee: GasAmount(0),
    };
    let charged_resources = &charged_resources_without_inner_calls
        + &CallInfo::summarize_charged_resources(syscall_handler.base.inner_calls.iter());

    Ok(CallInfo {
        call: syscall_handler.base.call,
        execution: CallExecution {
            retdata: Retdata(call_result.return_values),
            events: syscall_handler.base.events,
            l2_to_l1_messages: syscall_handler.base.l2_to_l1_messages,
            failed: call_result.failure_flag,
            gas_consumed,
        },
        charged_resources,
        inner_calls: syscall_handler.base.inner_calls,
        storage_read_values: syscall_handler.base.read_values,
        accessed_storage_keys: syscall_handler.base.accessed_keys,
        accessed_contract_addresses: syscall_handler.base.accessed_contract_addresses,
        read_class_hash_values: syscall_handler.base.read_class_hash_values,
        tracked_resource: TrackedResource::SierraGas,
    })
}
