//! Dispatches a [`NativeContractExecutor`] entry-point call, transparently routing
//! through `AotWithProgram::run_with_profile` when libfunc profiling is enabled.
//!
//! Keeps the libfunc-profiling cfg-noise out of the main entry-point execution path.

use cairo_native::error::Result;
use cairo_native::execution_result::ContractExecutionResult;
use cairo_native::utils::BuiltinCosts;
use starknet_types_core::felt::Felt;

use crate::execution::native::contract_class::NativeContractExecutor;
use crate::execution::native::syscall_handler::NativeSyscallHandler;

/// Runs an entry point on `executor`. Always available.
///
/// All [`NativeContractExecutor`] candidates expose the same `run` shape, so the call
/// site is identical across builds. When `with-libfunc-profiling` is enabled (and
/// `sierra-emu` is not -- the interpreter takes precedence and collects no profiles),
/// the call is routed through `AotWithProgram::run_with_profile`, which invokes our
/// callback with the captured profile and the program the executor was built from; we
/// record it into [`crate::execution::native::profiling::LIBFUNC_PROFILES_MAP`] keyed
/// by the current transaction hash.
pub fn run_native_executor(
    executor: &NativeContractExecutor,
    selector: Felt,
    calldata: &[Felt],
    call_initial_gas: u64,
    builtin_costs: BuiltinCosts,
    syscall_handler: &mut NativeSyscallHandler<'_>,
) -> Result<ContractExecutionResult> {
    #[cfg(all(feature = "with-libfunc-profiling", not(feature = "sierra-emu")))]
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
    #[cfg(any(not(feature = "with-libfunc-profiling"), feature = "sierra-emu"))]
    executor.run(selector, calldata, call_initial_gas, Some(builtin_costs), syscall_handler)
}
