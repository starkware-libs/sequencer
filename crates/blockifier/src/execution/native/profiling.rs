use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::{LazyLock, Mutex};

use cairo_lang_sierra::ids::ConcreteLibfuncId;
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
    pub program: cairo_lang_sierra::program::Program,
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
    program: &cairo_lang_sierra::program::Program,
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

    let trace_id = unsafe {
        let trace_id_ptr = executor.find_symbol_ptr(ProfilerBinding::ProfileId.symbol()).unwrap();
        trace_id_ptr.cast::<u64>().as_mut().unwrap()
    };

    let old_trace_id = *trace_id;
    *trace_id = counter;

    let result = executor.run(selector, args, gas, builtin_costs, syscall_handler);

    let profiler = LIBFUNC_PROFILE.lock().unwrap().remove(&counter).unwrap();
    let raw_profile = profiler.get_profile(program);

    let mut profiles_map = LIBFUNC_PROFILES_MAP.lock().unwrap();

    let profile =
        EntrypointProfile { class_hash, selector, profile: raw_profile, program: program.clone() };

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

    *trace_id = old_trace_id;

    result
}
