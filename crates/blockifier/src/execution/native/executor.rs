#[cfg(feature = "with-libfunc-profiling")]
use std::sync::atomic::AtomicU64;

use cairo_native::execution_result::ContractExecutionResult;
use cairo_native::executor::AotContractExecutor;
use cairo_native::utils::BuiltinCosts;
use starknet_types_core::felt::Felt;
#[cfg(feature = "with-libfunc-profiling")]
use {
    cairo_lang_sierra::ids::ConcreteLibfuncId,
    cairo_native::metadata::profiler::LibfuncProfileData,
    std::collections::HashMap,
    std::sync::{LazyLock, Mutex},
};

use crate::execution::native::syscall_handler::NativeSyscallHandler;

#[cfg(feature = "with-libfunc-profiling")]
pub struct EntrypointProfile {
    pub class_hash: Felt,
    pub selector: Felt,
    pub profile: HashMap<ConcreteLibfuncId, LibfuncProfileData>,
    pub program: cairo_lang_sierra::program::Program,
}

#[cfg(feature = "with-libfunc-profiling")]
pub struct TransactionProfile {
    pub block_number: u64,
    pub tx_hash: String,
    pub entrypoint_profiles: Vec<EntrypointProfile>,
}

#[cfg(feature = "with-libfunc-profiling")]
type ProfilesByBlockTx = HashMap<String, TransactionProfile>;

#[cfg(feature = "with-libfunc-profiling")]
pub static LIBFUNC_PROFILES_MAP: LazyLock<Mutex<ProfilesByBlockTx>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug)]
pub enum ContractExecutor {
    Aot(AotContractExecutor),
    #[cfg(feature = "with-libfunc-profiling")]
    AotWithProgram((AotContractExecutor, cairo_lang_sierra::program::Program)),
}

impl From<AotContractExecutor> for ContractExecutor {
    fn from(value: AotContractExecutor) -> Self {
        Self::Aot(value)
    }
}

impl ContractExecutor {
    pub fn run(
        &self,
        selector: Felt,
        args: &[Felt],
        gas: u64,
        builtin_costs: Option<BuiltinCosts>,
        syscall_handler: &mut NativeSyscallHandler<'_>,
    ) -> cairo_native::error::Result<ContractExecutionResult> {
        match self {
            ContractExecutor::Aot(aot_contract_executor) => {
                aot_contract_executor.run(selector, args, gas, builtin_costs, syscall_handler)
            }
            #[cfg(feature = "with-libfunc-profiling")]
            ContractExecutor::AotWithProgram((executor, program)) => {
                use cairo_native::metadata::profiler::{
                    ProfilerBinding,
                    ProfilerImpl,
                    LIBFUNC_PROFILE,
                };

                static COUNTER: AtomicU64 = AtomicU64::new(0);

                let libfunc_profiling_trace_id: &mut u64;
                let libfunc_profiling_old_trace_id: u64;
                let class_hash = *syscall_handler.base.call.class_hash;
                let tx_hash = syscall_handler
                    .base
                    .context
                    .tx_context
                    .tx_info
                    .transaction_hash()
                    .to_hex_string();
                let block_number =
                    syscall_handler.base.context.tx_context.block_context.block_info.block_number.0;

                let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                LIBFUNC_PROFILE.lock().unwrap().insert(counter, ProfilerImpl::new());

                libfunc_profiling_trace_id = unsafe {
                    let trace_id_ptr =
                        executor.find_symbol_ptr(ProfilerBinding::ProfileId.symbol()).unwrap();
                    trace_id_ptr.cast::<u64>().as_mut().unwrap()
                };

                libfunc_profiling_old_trace_id = *libfunc_profiling_trace_id;
                *libfunc_profiling_trace_id = counter;

                let result = executor.run(selector, args, gas, builtin_costs, syscall_handler);

                let profile = LIBFUNC_PROFILE.lock().unwrap().remove(&counter).unwrap();

                let raw_profile = profile.get_profile(program);

                let mut profiles_map = LIBFUNC_PROFILES_MAP.lock().unwrap();

                let profile = EntrypointProfile {
                    class_hash,
                    selector,
                    profile: raw_profile,
                    program: program.clone(),
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

                *libfunc_profiling_trace_id = libfunc_profiling_old_trace_id;

                result
            }
        }
    }
}
