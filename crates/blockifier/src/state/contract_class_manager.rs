use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::Arc;

use log::{error, info};
use starknet_api::core::ClassHash;
use starknet_api::state::ContractClass as SierraContractClass;
use starknet_sierra_compile::command_line_compiler::CommandLineCompiler;
use starknet_sierra_compile::config::SierraToCasmCompilationConfig;
use starknet_sierra_compile::SierraToNativeCompiler;

use crate::execution::contract_class::ContractClassV1;
use crate::execution::contract_class::RunnableContractClass;
use crate::execution::native::contract_class::NativeContractClassV1;
use crate::state::global_cache::{CachedCairoNative, ContractClassCaches};

const CHANNEL_SIZE: usize = 1000;

/// A compilation request is a tuple of the class hash, the sierra contract class, and the casm
/// contract class.
/// * `class_hash` - used to identify the contract class in the cache.
/// * `sierra_contract_class` - the code to be compiled.
/// * `casm_contract_class` - stored in [`NativeContractClassV1`] to allow fallback to cairo_vm
///   execution in case of unxecpected failure during native execution.
type CompilationRequest = (ClassHash, Arc<SierraContractClass>, ContractClassV1);

/// Manages the global cache of contract classes and handles sierra-to-native compilation requests.
struct ContractClassManager {
    // The global cache of contract classes: casm, sierra, and native.
    contract_class_caches: ContractClassCaches,
    // The sending half of the channel for sending compilation requests.
    sender: SyncSender<CompilationRequest>,
    // A flag that signals the termination of the compilation requests thread.
    halt_marker: AtomicBool,
}

impl ContractClassManager {
    /// Creates a new contract class manager and spawns a thread that listens for compilation
    /// requests and processes them. Returns an Arc to the created manager.
    pub fn initialize(contract_class_caches: ContractClassCaches) -> Arc<ContractClassManager> {
        // TODO(Avi, 15/12/2024): Add the size of the channel to the config.
        let (sender, receiver) = sync_channel(CHANNEL_SIZE);
        let compiler_config = SierraToCasmCompilationConfig::default();
        let compiler = CommandLineCompiler::new(compiler_config);
        let halt_marker = AtomicBool::new(false);
        let contract_class_manager =
            Arc::new(ContractClassManager { contract_class_caches, sender, halt_marker });

        std::thread::spawn({
            let contract_class_manager = Arc::clone(&contract_class_manager);
            move || contract_class_manager.compilation_requests_handler(receiver, compiler)
        });

        contract_class_manager
    }

    /// Tries to send a compilation request to the manager. Does not block the sender thread.
    /// If the channel is full, logs an error.
    pub fn try_send_compilation_request(&self, compilation_request: CompilationRequest) {
        let (class_hash, sierra, casm) = compilation_request.clone();
        self.contract_class_caches.set_sierra(class_hash, sierra);
        let cached_casm = RunnableContractClass::from(casm.clone());
        self.contract_class_caches.set_casm(class_hash, cached_casm);

        self.sender.try_send(compilation_request).map_err(|err| match err {
            TrySendError::Full(request) => error!(
                "Compilation request channel is full (size: {}). Compilation request for \
                 class_hash {:?} was not sent.",
                CHANNEL_SIZE, request.0
            ),
            TrySendError::Disconnected(_) => panic!("Compilation request channel is closed."),
        });
    }

    /// Halts the compilation requests thread.
    pub fn halt(&self) {
        self.halt_marker.store(true, Ordering::Relaxed);
    }

    pub fn get_native(&self, class_hash: &ClassHash) -> Option<CachedCairoNative> {
        self.contract_class_caches.get_native(class_hash)
    }

    pub fn get_sierra(&self, class_hash: &ClassHash) -> Option<Arc<SierraContractClass>> {
        self.contract_class_caches.get_sierra(class_hash)
    }

    pub fn get_casm(&self, class_hash: &ClassHash) -> Option<RunnableContractClass> {
        self.contract_class_caches.get_casm(class_hash)
    }

    /// Handles compilation requests from the receiver. For each request, compiles the sierra
    /// contract class to a native contract class and stores the result in the cache.
    /// If no request is available, non-busy-waits until a request is available.
    fn compilation_requests_handler(
        &self,
        receiver: Receiver<CompilationRequest>,
        compiler: CommandLineCompiler,
    ) {
        info!("Compilation requests handler started.");
        for (class_hash, sierra, casm) in receiver.iter() {
            if self.halted() {
                info!("Compilation requests handler terminated.");
                return;
            }
            if self.contract_class_caches.get_native(&class_hash).is_some() {
                // The contract class is already compiled to native - skip the compilation.
                continue;
            }
            // TODO(Avi): Convert `sierra_contract_class` to
            // `cairo_lang_starknet_classes::contract_class::ContractClass`
            let compilation_result = compiler.compile_to_native(sierra.into());
            match compilation_result {
                Ok(executor) => {
                    let native_contract_class = NativeContractClassV1::new(executor, casm);
                    self.contract_class_caches
                        .set_native(class_hash, CachedCairoNative::Compiled(native_contract_class));
                }
                Err(err) => {
                    error!("Error compiling contract class: {}", err);
                    self.contract_class_caches
                        .set_native(class_hash, CachedCairoNative::CompilationFailed);
                }
            }
        }
    }

    fn halted(&self) -> bool {
        self.halt_marker.load(Ordering::Relaxed)
    }
}
