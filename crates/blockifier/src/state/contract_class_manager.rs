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

/// Represents a request for the compilation handler.
///
/// # `Request` variants:
/// * `Terminate` - signals the compilation handler to terminate.
/// * `Compile` - requests compilation of a sierra contract class to a native compiled class.
///
/// # `Request::Compile` fields:
/// * `class_hash` - used to identify the contract class in the cache.
/// * `sierra_contract_class` - the sierra contract class to be compiled.
/// * `casm_contract_class` - stored in [`NativeContractClassV1`] to allow fallback to cairo_vm
///   execution in case of unxecpected failure during native execution.
pub enum Request {
    Terminate,
    Compile(ClassHash, Arc<SierraContractClass>, ContractClassV1),
}

impl Display for Request {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Request::Terminate => write!(f, "Terminate"),
            Request::Compile(class_hash, _, _) => {
                write!(f, "Request(class_hash: {:?})", class_hash)
            }
        }
    }
}

/// Manages the global cache of contract classes and handles sierra-to-native compilation requests.
struct ContractClassManager {
    // The global cache of contract classes: casm, sierra, and native.
    contract_class_caches: ContractClassCaches,
    // The sending half of the compilation request channel.
    sender: SyncSender<Request>,
    // A flag that signals the termination of the compilation handler.
    stop_marker: AtomicBool,
    // The join handle to the thread running the compilation handler.
    join_handle: Mutex<Option<JoinHandle<()>>>,
}

impl ContractClassManager {
    /// Creates a new contract class manager and spawns a thread that listens for compilation
    /// requests and processes them (a.k.a. the compilation handler).
    /// Returns an `Arc` to the created contract class manager.
    pub fn start(contract_class_caches: ContractClassCaches) -> Arc<ContractClassManager> {
        // TODO(Avi, 15/12/2024): Add the size of the channel to the config.
        let (sender, receiver) = sync_channel(CHANNEL_SIZE);
        let compiler_config = SierraToCasmCompilationConfig::default();
        let compiler = CommandLineCompiler::new(compiler_config);

        // Create the contract class manager.
        let contract_class_manager = Arc::new(ContractClassManager {
            contract_class_caches,
            sender,
            stop_marker: AtomicBool::new(false),
            // Store `None` in a mutex-guarded Option to allow setting the join handle after
            // spawning the thread.
            join_handle: Mutex::new(None),
        });

        // Spawn a thread running the compilation handler.
        let join_handle = std::thread::spawn({
            let contract_class_manager = Arc::clone(&contract_class_manager);
            move || contract_class_manager.run_compilation_handler(receiver, compiler)
        });

        // Store the join handle to allow waiting for the thread to finish.
        let mut mutex_guard = contract_class_manager
            .join_handle
            .lock()
            .expect("No other thread should access the join handle.");
        *mutex_guard = Some(join_handle);
        drop(mutex_guard);

        // Return the contract class manager.
        contract_class_manager
    }

    /// Stops the compilation handler.
    pub fn stop(&self) {
        self.stop_marker.store(true, Ordering::Relaxed);
        // For cases where the compilation request channel is empty, send a termination request to
        // unblock the compilation handler. This is necessary because the handler is
        // non-busy-waiting for requests.
        self.send_request(Request::Terminate);
    }

    /// Sends a request to the compilation handler.
    pub fn send_request(&self, request: Request) {
        self.cache_request_contracts(&request);

        match request {
            Request::Terminate => {
                // When sending a termination request, block the sender until the request is sent.
                self.sender.send(request).expect("Compilation request channel is closed.");
            }
            Request::Compile(_, _, _) => {
                // When sending a compilation request, send the request without blocking the sender.
                // TODO(Avi, 15/12/2024): Check for duplicated requests.
                self.sender.try_send(request).map_err(|err| match err {
                    TrySendError::Full(request) => {
                        error!(
                            "Compilation request channel is full (size: {}). Compilation request \
                             {} was not sent.",
                            CHANNEL_SIZE, request
                        )
                    }
                    TrySendError::Disconnected(_) => {
                        panic!("Compilation request channel is closed.")
                    }
                });
            }
        }
    }

    /// Returns the native compiled class for the given class hash, if it exists in cache.
    pub fn get_native(&self, class_hash: &ClassHash) -> Option<CachedCairoNative> {
        self.contract_class_caches.get_native(class_hash)
    }

    /// Returns the Sierra contract class for the given class hash, if it exists in cache.
    pub fn get_sierra(&self, class_hash: &ClassHash) -> Option<Arc<SierraContractClass>> {
        self.contract_class_caches.get_sierra(class_hash)
    }

    /// Returns the casm compiled class for the given class hash, if it exists in cache.
    pub fn get_casm(&self, class_hash: &ClassHash) -> Option<RunnableContractClass> {
        self.contract_class_caches.get_casm(class_hash)
    }

    /// Handles requests on the compilation request channel.
    /// If no request is available, non-busy-waits until a request is available.
    fn run_compilation_handler(&self, receiver: Receiver<Request>, compiler: CommandLineCompiler) {
        info!("Compilation handler started.");
        for request in receiver.iter() {
            if self.stopped() {
                info!("Compilation handler terminated.");
                return;
            }
            match request {
                Request::Terminate => {
                    info!("Compilation handler terminated.");
                    return;
                }
                Request::Compile(class_hash, sierra, casm) => {
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

    /// Caches the sierra and casm contract classes of a compilation request.
    fn cache_request_contracts(&self, request: &Request) {
        match request {
            Request::Terminate => {}
            Request::Compile(class_hash, sierra, casm) => {
                self.contract_class_caches.set_sierra(class_hash.clone(), sierra.clone());
                let cached_casm = RunnableContractClass::from(casm.clone());
                self.contract_class_caches.set_casm(class_hash.clone(), cached_casm);
            }
        }
    }

    /// Returns true if the compilation handler has been stopped.
    fn stopped(&self) -> bool {
        self.stop_marker.load(Ordering::Relaxed)
    }
}

impl Drop for ContractClassManager {
    /// Ensures the thread running the compilation handler is terminated when the contract class
    /// manager is dropped.
    fn drop(&mut self) {
        self.stop();
        let join_handle = self
            .join_handle
            .lock()
            .expect("The lock should only be accessed when the contract class manager is dropped.")
            .take()
            .expect(
                "The join handle should be set when the thread running the compilation handler is 
            spawned.",
            );

        join_handle.join().unwrap();
    }
}
