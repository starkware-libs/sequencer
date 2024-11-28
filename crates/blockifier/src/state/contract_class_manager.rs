use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread::JoinHandle;

use log::{error, info};
use starknet_api::core::ClassHash;
use starknet_api::state::ContractClass as SierraContractClass;
use starknet_sierra_compile::command_line_compiler::CommandLineCompiler;
use starknet_sierra_compile::config::SierraToCasmCompilationConfig;
use starknet_sierra_compile::SierraToNativeCompiler;

use crate::execution::contract_class::{ContractClassV1, RunnableContractClass};
use crate::execution::native::contract_class::NativeContractClassV1;
use crate::state::global_cache::{CachedCairoNative, ContractCaches};

const CHANNEL_SIZE: usize = 1000;

/// Represents a request to compile a sierra contract class to a native compiled class.
///
/// # Fields:
/// * `class_hash` - used to identify the contract class in the cache.
/// * `sierra_contract_class` - the sierra contract class to be compiled.
/// * `casm_compiled_class` - stored in [`NativeContractClassV1`] to allow fallback to cairo_vm
///   execution in case of unxecpected failure during native execution.
type CompilationRequest = (ClassHash, Arc<SierraContractClass>, ContractClassV1);

/// Manages the global cache of contract classes and handles sierra-to-native compilation requests.
struct ContractClassManager {
    // The global cache of contract classes: casm, sierra, and native.
    contract_caches: Arc<ContractCaches>,
    // The sending half of the compilation request channel.
    sender: SyncSender<CompilationRequest>,
    // A flag that signals the termination of the compilation handler.
    stop_marker: Arc<AtomicBool>,
    // The join handle to the thread running the compilation handler.
    join_handle: JoinHandle<()>,
}

impl ContractClassManager {
    /// Creates a new contract class manager and spawns a thread that listens for compilation
    /// requests and processes them (a.k.a. the compilation handler).
    /// Returns the contract class manager.
    pub fn start(contract_caches: ContractCaches) -> ContractClassManager {
        // TODO(Avi, 15/12/2024): Add the size of the channel to the config.
        let contract_caches = Arc::new(contract_caches);
        let (sender, receiver) = sync_channel(CHANNEL_SIZE);
        let compiler_config = SierraToCasmCompilationConfig::default();
        let compiler = CommandLineCompiler::new(compiler_config);
        let stop_marker = Arc::new(AtomicBool::new(false));

        let join_handle = std::thread::spawn({
            let contract_caches = Arc::clone(&contract_caches);
            let stop_marker = Arc::clone(&stop_marker);
            move || run_compilation_handler(contract_caches, receiver, compiler, stop_marker)
        });

        ContractClassManager { contract_caches, sender, stop_marker, join_handle }
    }

    /// Stops the compilation handler.
    pub fn stop(&self) {
        self.stop_marker.store(true, Ordering::Relaxed);
    }

    /// Sends a compilation request to the compilation handler. Does not block the sender. Logs an
    /// error is the channel is full.
    pub fn send_compilation_request(&self, request: CompilationRequest) {
        self.cache_request_contracts(&request);
        // TODO(Avi, 15/12/2024): Check for duplicated requests.
        self.sender.try_send(request).map_err(|err| match err {
            TrySendError::Full((class_hash, _, _)) => {
                error!(
                    "Compilation request channel is full (size: {}). Compilation request for \
                     class hash {} was not sent.",
                    CHANNEL_SIZE, class_hash
                )
            }
            TrySendError::Disconnected(_) => {
                panic!("Compilation request channel is closed.")
            }
        });
    }

    /// Returns the native compiled class for the given class hash, if it exists in cache.
    pub fn get_native(&self, class_hash: &ClassHash) -> Option<CachedCairoNative> {
        self.contract_caches.get_native(class_hash)
    }

    /// Returns the Sierra contract class for the given class hash, if it exists in cache.
    pub fn get_sierra(&self, class_hash: &ClassHash) -> Option<Arc<SierraContractClass>> {
        self.contract_caches.get_sierra(class_hash)
    }

    /// Returns the casm compiled class for the given class hash, if it exists in cache.
    pub fn get_casm(&self, class_hash: &ClassHash) -> Option<RunnableContractClass> {
        self.contract_caches.get_casm(class_hash)
    }

    /// Waits for the compilation handler to terminate.
    pub fn join(self) {
        self.join_handle.join().unwrap();
    }

    /// Caches the sierra and casm contract classes of a compilation request.
    fn cache_request_contracts(&self, request: &CompilationRequest) {
        let (class_hash, sierra, casm) = request.clone();
        self.contract_caches.set_sierra(class_hash, sierra);
        let cached_casm = RunnableContractClass::from(casm);
        self.contract_caches.set_casm(class_hash, cached_casm);
    }
}

/// Handles compilation requests from the channel.
/// If no request is available, non-busy-waits until a request is available.
/// When the sender is dropped, the compilation handler processes all pending requests and
/// terminates.
fn run_compilation_handler(
    contract_caches: Arc<ContractCaches>,
    receiver: Receiver<CompilationRequest>,
    compiler: CommandLineCompiler,
    stop_marker: Arc<AtomicBool>,
) {
    info!("Compilation handler started.");
    for (class_hash, sierra, casm) in receiver.iter() {
        if stop_marker.load(Ordering::Relaxed) {
            info!("Compilation handler terminated.");
            return;
        }
        if contract_caches.get_native(&class_hash).is_some() {
            // The contract class is already compiled to native - skip the compilation.
            continue;
        }
        // TODO(Avi): Convert `sierra_contract_class` to
        // `cairo_lang_starknet_classes::contract_class::ContractClass`
        let compilation_result = compiler.compile_to_native(sierra.into());
        match compilation_result {
            Ok(executor) => {
                let native_compiled_class = NativeContractClassV1::new(executor, casm);
                contract_caches
                    .set_native(class_hash, CachedCairoNative::Compiled(native_compiled_class));
            }
            Err(err) => {
                error!("Error compiling contract class: {}", err);
                contract_caches.set_native(class_hash, CachedCairoNative::CompilationFailed);
            }
        }
    }
}
