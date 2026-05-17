//! Blockifier-side storage for libfunc profiles collected by
//! [`cairo_native::ContractExecutor::run_with_profile`].
//!
//! cairo-native owns the profiling primitive; this module only provides the keying
//! (transaction hash / block number / class hash + selector) that's not visible from
//! cairo-native's layer.
//!
//! Intended for single-tenant benchmarking. The map grows without bound — drain it
//! externally between runs.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use cairo_lang_sierra::program::Program;
use cairo_native::metadata::profiler::Profile;
use starknet_types_core::felt::Felt;

use crate::execution::native::syscall_handler::NativeSyscallHandler;

pub struct EntrypointProfile {
    pub class_hash: Felt,
    pub selector: Felt,
    pub profile: Profile,
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

/// Builds an `FnOnce(Profile)` that, when invoked, records the captured profile in
/// `LIBFUNC_PROFILES_MAP` keyed by the syscall handler's current transaction hash.
///
/// All keying data is extracted up front so the closure doesn't re-borrow
/// `syscall_handler` — required because the closure outlives the call to
/// `ContractExecutor::run_with_profile`, which itself holds a `&mut` to the handler.
pub fn record_profile_for(
    syscall_handler: &NativeSyscallHandler<'_>,
    selector: Felt,
    program: Arc<Program>,
) -> impl FnOnce(Profile) + 'static {
    let class_hash = *syscall_handler.base.call.class_hash;
    let tx_hash =
        syscall_handler.base.context.tx_context.tx_info.transaction_hash().to_hex_string();
    let block_number =
        syscall_handler.base.context.tx_context.block_context.block_info.block_number.0;

    move |profile| {
        let entry = EntrypointProfile { class_hash, selector, profile, program };
        let mut map = LIBFUNC_PROFILES_MAP.lock().unwrap();
        map.entry(tx_hash.clone())
            .or_insert_with(|| TransactionProfile {
                block_number,
                tx_hash: tx_hash.clone(),
                entrypoint_profiles: Vec::new(),
            })
            .entrypoint_profiles
            .push(entry);
    }
}
