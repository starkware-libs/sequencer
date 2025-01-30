#[cfg(feature = "cairo_native")]
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
#[cfg(feature = "cairo_native")]
use std::sync::Arc;

#[cfg(any(feature = "testing", test))]
use cached::Cached;
#[cfg(feature = "cairo_native")]
use log;
use starknet_api::core::ClassHash;
#[cfg(feature = "cairo_native")]
use starknet_api::state::SierraContractClass;
#[cfg(feature = "cairo_native")]
use starknet_sierra_multicompile::command_line_compiler::CommandLineCompiler;
#[cfg(feature = "cairo_native")]
use starknet_sierra_multicompile::utils::into_contract_class_for_compilation;
#[cfg(feature = "cairo_native")]
use starknet_sierra_multicompile::SierraToNativeCompiler;

#[cfg(feature = "cairo_native")]
use crate::blockifier::config::CairoNativeRunConfig;
use crate::blockifier::config::ContractClassManagerConfig;
#[cfg(feature = "cairo_native")]
use crate::execution::contract_class::CompiledClassV1;
#[cfg(feature = "cairo_native")]
use crate::execution::native::contract_class::NativeCompiledClassV1;
#[cfg(feature = "cairo_native")]
use crate::state::global_cache::CachedCairoNative;
use crate::state::global_cache::{CachedCasm, ContractCaches};
pub const DEFAULT_COMPILATION_REQUEST_CHANNEL_SIZE: usize = 1000;

/// Represents a request to compile a sierra contract class to a native compiled class.
///
/// # Fields:
/// * `class_hash` - used to identify the contract class in the cache.
/// * `sierra_contract_class` - the sierra contract class to be compiled.
/// * `casm_compiled_class` - stored in [`NativeCompiledClassV1`] to allow fallback to cairo_vm
///   execution in case of unexpected failure during native execution.
#[cfg(feature = "cairo_native")]
type CompilationRequest = (ClassHash, Arc<SierraContractClass>, CompiledClassV1);

/// Manages the global cache of contract classes and handles sierra-to-native compilation requests.
#[derive(Clone)]
pub struct ContractClassManager {
    #[cfg(feature = "cairo_native")]
    cairo_native_run_config: CairoNativeRunConfig,
    /// The global cache of contract classes: casm, sierra, and native.
    contract_caches: ContractCaches,
    /// The sending half of the compilation request channel. Set to `None` if native compilation is
    /// disabled.
    #[cfg(feature = "cairo_native")]
    sender: Option<SyncSender<CompilationRequest>>,
    /// The sierra-to-native compiler.
    #[cfg(feature = "cairo_native")]
    compiler: Option<Arc<dyn SierraToNativeCompiler>>,
}

impl ContractClassManager {
    /// Creates a new contract class manager and spawns a thread that listens for compilation
    /// requests and processes them (a.k.a. the compilation worker).
    /// Returns the contract class manager.
    /// NOTE: the compilation worker is not spawned if one of the following conditions is met:
    /// 1. The feature `cairo_native` is not enabled.
    /// 2. `config.run_cairo_native` is `false`.
    /// 3. `config.wait_on_native_compilation` is `true`.
    pub fn start(config: ContractClassManagerConfig) -> ContractClassManager {
        // TODO(Avi, 15/12/2024): Add the size of the channel to the config.
        let contract_caches = ContractCaches::new(config.contract_cache_size);
        #[cfg(not(feature = "cairo_native"))]
        return ContractClassManager { contract_caches };
        #[cfg(feature = "cairo_native")]
        {
            let cairo_native_run_config = config.cairo_native_run_config;
            if !cairo_native_run_config.run_cairo_native {
                // Native compilation is disabled - no need to start the compilation worker.
                return ContractClassManager {
                    cairo_native_run_config: config.cairo_native_run_config,
                    contract_caches,
                    sender: None,
                    compiler: None,
                };
            }

            let compiler_config = config.native_compiler_config.clone();
            let compiler = Arc::new(CommandLineCompiler::new(compiler_config));
            if cairo_native_run_config.wait_on_native_compilation {
                // Compilation requests are processed synchronously. No need to start the worker.
                return ContractClassManager {
                    cairo_native_run_config,
                    contract_caches,
                    sender: None,
                    compiler: Some(compiler),
                };
            }

            let (sender, receiver) = sync_channel(cairo_native_run_config.channel_size);

            std::thread::spawn({
                let contract_caches = contract_caches.clone();
                move || run_compilation_worker(contract_caches, receiver, compiler)
            });

            ContractClassManager {
                cairo_native_run_config,
                contract_caches,
                sender: Some(sender),
                compiler: None,
            }
        }
    }

    /// Sends a compilation request. Two cases:
    /// 1. If `config.wait_on_native_compilation` is `false`, sends the request to the compilation
    ///    worker. Does not block the sender. Logs an error if the channel is full.
    /// 2. Otherwise, processes the request synchronously, blocking the sender until the request is
    ///    processed.
    #[cfg(feature = "cairo_native")]
    pub fn send_compilation_request(&self, request: CompilationRequest) {
        assert!(self.run_cairo_native(), "Native compilation is disabled.");
        if self.wait_on_native_compilation() {
            // Compilation requests are processed synchronously. No need to go through the channel.
            let compiler = self.compiler.as_ref().expect("Compiler not available.");
            process_compilation_request(self.contract_caches.clone(), compiler.clone(), request);
            return;
        }

        let sender = self.sender.as_ref().expect("Compilation channel not available.");
        // TODO(Avi, 15/12/2024): Check for duplicated requests.
        sender.try_send(request).unwrap_or_else(|err| match err {
            TrySendError::Full((class_hash, _, _)) => {
                log::error!(
                    "Compilation request channel is full (size: {}). Compilation request for \
                     class hash {} was not sent.",
                    self.cairo_native_run_config.channel_size,
                    class_hash
                )
            }
            TrySendError::Disconnected(_) => {
                panic!("Compilation request channel is closed.")
            }
        });
    }

    /// Returns the native compiled class for the given class hash, if it exists in cache.
    #[cfg(feature = "cairo_native")]
    pub fn get_native(&self, class_hash: &ClassHash) -> Option<CachedCairoNative> {
        self.contract_caches.get_native(class_hash)
    }

    /// Returns the casm compiled class for the given class hash, if it exists in cache.
    pub fn get_casm(&self, class_hash: &ClassHash) -> Option<CachedCasm> {
        self.contract_caches.get_casm(class_hash)
    }

    /// Sets the casm compiled class for the given class hash in the cache.
    pub fn set_casm(&self, class_hash: ClassHash, compiled_class: CachedCasm) {
        self.contract_caches.set_casm(class_hash, compiled_class);
    }

    #[cfg(all(feature = "cairo_native", feature = "testing"))]
    pub fn set_native(&self, class_hash: ClassHash, compiled_class: NativeCompiledClassV1) {
        self.contract_caches.set_native(class_hash, CachedCairoNative::Compiled(compiled_class));
    }

    #[cfg(feature = "cairo_native")]
    pub fn run_cairo_native(&self) -> bool {
        self.cairo_native_run_config.run_cairo_native
    }

    #[cfg(feature = "cairo_native")]
    pub fn wait_on_native_compilation(&self) -> bool {
        self.cairo_native_run_config.wait_on_native_compilation
    }

    /// Clear the contract caches.
    pub fn clear(&mut self) {
        self.contract_caches.clear();
    }

    #[cfg(any(feature = "testing", test))]
    pub fn get_casm_cache_size(&self) -> usize {
        self.contract_caches.casm_cache.lock().cache_size()
    }
}

/// Handles compilation requests from the channel, holding the receiver end of the channel.
/// If no request is available, non-busy-waits until a request is available.
/// When the sender is dropped, the worker processes all pending requests and terminates.
#[cfg(feature = "cairo_native")]
fn run_compilation_worker(
    contract_caches: ContractCaches,
    receiver: Receiver<CompilationRequest>,
    compiler: Arc<dyn SierraToNativeCompiler>,
) {
    log::info!("Compilation worker started.");
    for compilation_request in receiver.iter() {
        process_compilation_request(contract_caches.clone(), compiler.clone(), compilation_request);
    }
    log::info!("Compilation worker terminated.");
}

/// Processes a compilation request and caches the compiled class in the contract caches.
#[cfg(feature = "cairo_native")]
fn process_compilation_request(
    contract_caches: ContractCaches,
    compiler: Arc<dyn SierraToNativeCompiler>,
    compilation_request: CompilationRequest,
) {
    let (class_hash, sierra, casm) = compilation_request;
    if contract_caches.get_native(&class_hash).is_some() {
        // The contract class is already compiled to native - skip the compilation.
        return;
    }
    let sierra_for_compilation = into_contract_class_for_compilation(sierra.as_ref());
    let compilation_result = compiler.compile_to_native(sierra_for_compilation);
    match compilation_result {
        Ok(executor) => {
            let native_compiled_class = NativeCompiledClassV1::new(executor.into(), casm);
            contract_caches
                .set_native(class_hash, CachedCairoNative::Compiled(native_compiled_class));
        }
        Err(err) => {
            log::error!("Error compiling contract class: {}", err);
            contract_caches.set_native(class_hash, CachedCairoNative::CompilationFailed);
        }
    }
}
