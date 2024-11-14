use cairo_native::execution_result::ContractExecutionResult;
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use num_traits::ToPrimitive;
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
    resources: &mut ExecutionResources,
    context: &mut EntryPointExecutionContext,
) -> EntryPointExecutionResult<CallInfo> {
    let entry_point = contract_class.get_entry_point(&call)?;

    let mut syscall_handler: NativeSyscallHandler<'_> =
        NativeSyscallHandler::new(call, state, resources, context);

    // // todo(rodrigo): using builtin cost messes up heavily with gas costs. Should we keep them
    // or not
    // let gas_costs = &syscall_handler.context.versioned_constants().os_constants.gas_costs;
    // let builtin_costs = BuiltinCosts {
    //     // todo(rodrigo): Unsure of what value `const` should have
    //     r#const: 0,
    //     pedersen: gas_costs.pedersen_gas_cost,
    //     bitwise: gas_costs.bitwise_builtin_gas_cost,
    //     ecop: gas_costs.ecop_gas_cost,
    //     poseidon: gas_costs.poseidon_gas_cost,
    //     add_mod: gas_costs.add_mod_gas_cost,
    //     mul_mod: gas_costs.mul_mod_gas_cost,
    // };

    let execution_result = contract_class.executor.run(
        entry_point.selector.0,
        &syscall_handler.call.calldata.0.clone(),
        Some(syscall_handler.call.initial_gas.into()),
        None,
        &mut syscall_handler,
    );

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
    let remaining_gas =
        call_result.remaining_gas.to_u64().ok_or(PostExecutionError::MalformedReturnData {
            error_message: format!(
                "Unexpected remaining gas. Gas value is bigger than u64: {}",
                call_result.remaining_gas
            ),
        })?;

    if remaining_gas > syscall_handler.call.initial_gas {
        return Err(PostExecutionError::MalformedReturnData {
            error_message: format!(
                "Unexpected remaining gas. Used gas is greater than initial gas: {} > {}",
                remaining_gas, syscall_handler.call.initial_gas
            ),
        }
        .into());
    }

    let gas_consumed = syscall_handler.call.initial_gas - remaining_gas;

    Ok(CallInfo {
        call: syscall_handler.call,
        execution: CallExecution {
            retdata: Retdata(call_result.return_values),
            events: syscall_handler.events,
            l2_to_l1_messages: syscall_handler.l2_to_l1_messages,
            failed: call_result.failure_flag,
            gas_consumed,
        },
        charged_resources: ChargedResources {
            vm_resources: ExecutionResources::default(),
            gas_for_fee: GasAmount(gas_consumed),
        },
        inner_calls: syscall_handler.inner_calls,
        storage_read_values: syscall_handler.read_values,
        accessed_storage_keys: syscall_handler.accessed_keys,
        // TODO(Aviv): The syscall is not supported here yet.
        // Currently, `accessed_contract_addresses` and `read_class_hash_values` are initialized
        // as empty. Support for handling accessed storage keys via syscalls should be implemented.
        accessed_contract_addresses: Default::default(),
        read_class_hash_values: Default::default(),
        tracked_resource: TrackedResource::SierraGas,
    })
}
