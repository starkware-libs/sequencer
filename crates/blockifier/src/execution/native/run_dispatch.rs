//! Dispatches a [`ContractExecutor`] entry-point call, transparently routing through
//! [`ContractExecutor::run_with_profile`] when libfunc profiling is enabled.
//!
//! Keeps the libfunc-profiling cfg-noise out of the main entry-point execution path.

use cairo_native::error::Result;
use cairo_native::execution_result::ContractExecutionResult;
use cairo_native::executor::ContractExecutor;
use cairo_native::utils::BuiltinCosts;
use starknet_types_core::felt::Felt;

use crate::execution::native::syscall_handler::NativeSyscallHandler;

/// Runs an entry point on `executor`. Always available.
///
/// When `with-libfunc-profiling` is enabled the call is routed through
/// [`ContractExecutor::run_with_profile`]; for the `AotWithProgram` variant cairo-native
/// invokes our callback with the captured profile and the program it was built from, which
/// we record into [`crate::execution::native::profiling::LIBFUNC_PROFILES_MAP`] keyed by
/// the current transaction hash. For other variants cairo-native falls through to
/// [`ContractExecutor::run`] and the callback is never invoked.
pub fn run_native_executor(
    executor: &ContractExecutor,
    selector: Felt,
    calldata: &[Felt],
    call_initial_gas: u64,
    builtin_costs: BuiltinCosts,
    syscall_handler: &mut NativeSyscallHandler<'_>,
) -> Result<ContractExecutionResult> {
    #[cfg(feature = "with-libfunc-profiling")]
    {
        let on_profile =
            crate::execution::native::profiling::record_profile_for(syscall_handler, selector);
        executor.run_with_profile(
            selector,
            calldata,
            call_initial_gas,
            Some(builtin_costs),
            syscall_handler,
            on_profile,
        )
    }
    #[cfg(not(feature = "with-libfunc-profiling"))]
    executor.run(selector, calldata, call_initial_gas, Some(builtin_costs), syscall_handler)
}
