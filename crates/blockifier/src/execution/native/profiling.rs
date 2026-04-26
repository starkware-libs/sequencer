use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, LazyLock, Mutex};

use cairo_lang_sierra::ids::ConcreteLibfuncId;
use cairo_lang_sierra::program::Program;
use cairo_native::execution_result::ContractExecutionResult;
use cairo_native::executor::AotContractExecutor;
use cairo_native::metadata::profiler::{
    LibfuncProfileData,
    ProfilerBinding,
    ProfilerImpl,
    LIBFUNC_PROFILE,
};
use cairo_native::utils::BuiltinCosts;
use starknet_types_core::felt::Felt;

use crate::execution::native::syscall_handler::NativeSyscallHandler;

pub struct EntrypointProfile {
    pub class_hash: Felt,
    pub selector: Felt,
    pub profile: HashMap<ConcreteLibfuncId, LibfuncProfileData>,
    pub program: Arc<Program>,
}

pub struct TransactionProfile {
    pub block_number: u64,
    pub tx_hash: String,
    pub entrypoint_profiles: Vec<EntrypointProfile>,
}

type ProfilesByBlockTx = HashMap<String, TransactionProfile>;

pub static LIBFUNC_PROFILES_MAP: LazyLock<Mutex<ProfilesByBlockTx>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Wraps `AotContractExecutor::run` with libfunc profiling instrumentation.
///
/// Collects per-entrypoint profiling data into `LIBFUNC_PROFILES_MAP`, keyed by transaction hash.
pub fn run_profiled(
    executor: &AotContractExecutor,
    program: &Arc<Program>,
    selector: Felt,
    args: &[Felt],
    gas: u64,
    builtin_costs: Option<BuiltinCosts>,
    syscall_handler: &mut NativeSyscallHandler<'_>,
) -> cairo_native::error::Result<ContractExecutionResult> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let class_hash = *syscall_handler.base.call.class_hash;
    let tx_hash =
        syscall_handler.base.context.tx_context.tx_info.transaction_hash().to_hex_string();
    let block_number =
        syscall_handler.base.context.tx_context.block_context.block_info.block_number.0;

    let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    LIBFUNC_PROFILE.lock().unwrap().insert(counter, ProfilerImpl::new());

    // `trace_id_ptr` targets a global symbol in the executor's shared library; the pointer
    // is valid for the executor's lifetime. Profiling is intended for single-threaded
    // benchmarking — concurrent calls would race on this symbol (tracked separately).
    let trace_id_ptr =
        executor.find_symbol_ptr(ProfilerBinding::ProfileId.symbol()).unwrap().cast::<u64>();
    // SAFETY: see above. Read/write to a non-null, properly-aligned `*mut u64`.
    let old_trace_id = unsafe { *trace_id_ptr };
    unsafe {
        *trace_id_ptr = counter;
    }

    // Restores `trace_id` and drops the `LIBFUNC_PROFILE` slot if `executor.run` (or any
    // step before the success-path drain) panics; without this the profiler symbol stays
    // pinned at `counter` and the slot leaks forever.
    let _guard = ProfilerGuard { trace_id_ptr, old_trace_id, counter };

    let result = executor.run(selector, args, gas, builtin_costs, syscall_handler);

    let profiler = LIBFUNC_PROFILE.lock().unwrap().remove(&counter).unwrap();
    let raw_profile = profiler.get_profile(program);

    let mut profiles_map = LIBFUNC_PROFILES_MAP.lock().unwrap();

    let profile = EntrypointProfile {
        class_hash,
        selector,
        profile: raw_profile,
        program: Arc::clone(program),
    };

    match profiles_map.get_mut(&tx_hash) {
        Some(tx_profile) => {
            tx_profile.entrypoint_profiles.push(profile);
        }
        None => {
            let tx_profile = TransactionProfile {
                block_number,
                tx_hash: tx_hash.clone(),
                entrypoint_profiles: vec![profile],
            };
            profiles_map.insert(tx_hash, tx_profile);
        }
    };

    result
}

/// RAII cleanup for the profiler globals. On drop, restores `*trace_id_ptr` to `old_trace_id`
/// and removes the `LIBFUNC_PROFILE` entry at `counter`. Removing on the success path is a
/// no-op because the caller has already taken the slot out.
struct ProfilerGuard {
    trace_id_ptr: *mut u64,
    old_trace_id: u64,
    counter: u64,
}

impl Drop for ProfilerGuard {
    fn drop(&mut self) {
        // SAFETY: the pointer targets a global in the executor's shared library and outlives
        // this guard. Single-threaded profiling means no concurrent writer aliases it here.
        unsafe {
            *self.trace_id_ptr = self.old_trace_id;
        }
        // Tolerate a poisoned mutex silently — Drop must not panic.
        if let Ok(mut profile) = LIBFUNC_PROFILE.lock() {
            profile.remove(&self.counter);
        }
    }
}
