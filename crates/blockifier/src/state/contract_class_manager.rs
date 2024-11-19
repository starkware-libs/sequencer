use std::fmt::{Display, Formatter};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use log::{error, info};
use starknet_api::core::ClassHash;
use starknet_api::state::ContractClass as SierraContractClass;
use starknet_sierra_compile::command_line_compiler::CommandLineCompiler;
use starknet_sierra_compile::config::SierraToCasmCompilationConfig;
use starknet_sierra_compile::SierraToNativeCompiler;

use crate::execution::contract_class::{ContractClassV1, RunnableContractClass};
use crate::execution::native::contract_class::NativeContractClassV1;
use crate::state::global_cache::{CachedCairoNative, ContractClassCaches};

const CHANNEL_SIZE: usize = 1000;

/// Represents a request to compile a sierra contract class to a native contract class. The request
/// can be either a termination request or a compilation request. A termination request signals the
/// compilation requests handler thread to terminate. A (non-termination) compilation request is a
/// tuple of the class hash, the sierra contract class, and the casm contract class.
/// * `class_hash` - used to identify the contract class in the cache.
/// * `sierra_contract_class` - the code to be compiled.
/// * `casm_contract_class` - stored in [`NativeContractClassV1`] to allow fallback to cairo_vm
///   execution in case of unxecpected failure during native execution.
pub enum CompilationRequest {
    Terminate,
    Request(ClassHash, Arc<SierraContractClass>, ContractClassV1),
}

impl Display for CompilationRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CompilationRequest::Terminate => write!(f, "Terminate"),
            CompilationRequest::Request(class_hash, _, _) => {
                write!(f, "Request(class_hash: {:?})", class_hash)
            }
        }
    }
}

/// Manages the global cache of contract classes and handles sierra-to-native compilation requests.
struct ContractClassManager {
    // The global cache of contract classes: casm, sierra, and native.
    contract_class_caches: ContractClassCaches,
    // The sending half of the channel for sending compilation requests.
    sender: SyncSender<CompilationRequest>,
    // A flag that signals the termination of the compilation requests thread.
    halt_marker: AtomicBool,
    // The handle to the compilation requests thread.
    join_handle: Mutex<Option<JoinHandle<()>>>,
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
        // Store `None` in a mutex-guarded Option to allow setting the join handle after spawning
        // the thread.
        let join_handle = Mutex::new(None);

        // Create the manager.
        let contract_class_manager = Arc::new(ContractClassManager {
            contract_class_caches,
            sender,
            halt_marker,
            join_handle,
        });

        // Spawn the compilation requests handler thread.
        let join_handle = std::thread::spawn({
            let contract_class_manager = Arc::clone(&contract_class_manager);
            move || contract_class_manager.compilation_requests_handler(receiver, compiler)
        });

        // Store the join handle in a mutex-guarded Option to allow waiting for the thread to
        // finish.
        let mut mutex_guard = contract_class_manager
            .join_handle
            .lock()
            .expect("No other thread should access the join handle.");
        *mutex_guard = Some(join_handle);
        drop(mutex_guard);

        contract_class_manager
    }

    /// Tries to send a compilation request to the manager. Does not block the sender thread.
    /// If the channel is full, logs an error.
    pub fn try_send_compilation_request(&self, compilation_request: CompilationRequest) {
        self.cache_compilation_request_contracts(&compilation_request);

        self.sender.try_send(compilation_request).map_err(|err| match err {
            TrySendError::Full(request) => {
                error!(
                    "Compilation request channel is full (size: {}). Compilation request {} was \
                     not sent.",
                    CHANNEL_SIZE, request
                )
            }
            TrySendError::Disconnected(_) => panic!("Compilation request channel is closed."),
        });
    }

    /// Halts the compilation requests thread.
    pub fn halt(&self) {
        self.halt_marker.store(true, Ordering::Relaxed);
        // In case the channel is empty, send a termination request to unblock the receiver.
        self.sender.try_send(CompilationRequest::Terminate).unwrap();
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
        for compilation_request in receiver.iter() {
            if self.halted() {
                info!("Compilation requests handler terminated.");
                return;
            }
            match compilation_request {
                CompilationRequest::Terminate => {
                    info!(
                        "Compilation requests handler terminated without setting the halt marker."
                    );
                    return;
                }
                CompilationRequest::Request(class_hash, sierra, casm) => {
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
                            self.contract_class_caches.set_native(
                                class_hash,
                                CachedCairoNative::Compiled(native_contract_class),
                            );
                        }
                        Err(err) => {
                            error!("Error compiling contract class: {}", err);
                            self.contract_class_caches
                                .set_native(class_hash, CachedCairoNative::CompilationFailed);
                        }
                    }
                }
            }
        }
    }

    fn cache_compilation_request_contracts(&self, compilation_request: &CompilationRequest) {
        match compilation_request {
            CompilationRequest::Terminate => {}
            CompilationRequest::Request(class_hash, sierra, casm) => {
                self.contract_class_caches.set_sierra(class_hash.clone(), sierra.clone());
                let cached_casm = RunnableContractClass::from(casm.clone());
                self.contract_class_caches.set_casm(class_hash.clone(), cached_casm);
            }
        }
    }

    fn halted(&self) -> bool {
        self.halt_marker.load(Ordering::Relaxed)
    }
}

impl Drop for ContractClassManager {
    fn drop(&mut self) {
        self.halt();
        let join_handle = self.join_handle.lock().unwrap().take().unwrap();
        join_handle.join().unwrap();
    }
}
