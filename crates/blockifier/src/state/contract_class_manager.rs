#[cfg(feature = "cairo_native")]
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
#[cfg(feature = "cairo_native")]
use std::sync::Arc;

#[cfg(any(feature = "testing", test))]
use cached::Cached;
#[cfg(feature = "cairo_native")]
use log::{error, info};
use starknet_api::core::ClassHash;
#[cfg(feature = "cairo_native")]
use starknet_api::state::SierraContractClass;
#[cfg(feature = "cairo_native")]
use starknet_sierra_compile::command_line_compiler::CommandLineCompiler;
#[cfg(feature = "cairo_native")]
use starknet_sierra_compile::config::SierraToCasmCompilationConfig;
#[cfg(feature = "cairo_native")]
use starknet_sierra_compile::utils::into_contract_class_for_compilation;
#[cfg(feature = "cairo_native")]
use starknet_sierra_compile::SierraToNativeCompiler;

use crate::blockifier::config::ContractClassManagerConfig;
#[cfg(feature = "cairo_native")]
use crate::execution::contract_class::CompiledClassV1;
use crate::execution::contract_class::RunnableCompiledClass;
#[cfg(feature = "cairo_native")]
use crate::execution::native::contract_class::NativeCompiledClassV1;
#[cfg(feature = "cairo_native")]
use crate::state::global_cache::CachedCairoNative;
use crate::state::global_cache::ContractCaches;

#[cfg(feature = "cairo_native")]
const CHANNEL_SIZE: usize = 1000;

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
    config: ContractClassManagerConfig,
    /// The global cache of contract classes: casm, sierra, and native.
    contract_caches: ContractCaches,
    /// The sending half of the compilation request channel. Set to `None` if native compilation is
    /// disabled.
    #[cfg(feature = "cairo_native")]
    sender: Option<SyncSender<CompilationRequest>>,
}

impl ContractClassManager {
    /// Creates a new contract class manager and spawns a thread that listens for compilation
    /// requests and processes them (a.k.a. the compilation worker).
    /// Returns the contract class manager.
    /// NOTE: If native compilation is disabled, the compilation worker is not spawned.
    pub fn start(config: ContractClassManagerConfig) -> ContractClassManager {
        // TODO(Avi, 15/12/2024): Add the size of the channel to the config.
        let contract_caches = ContractCaches::new(config.contract_cache_size);
        #[cfg(not(feature = "cairo_native"))]
        return ContractClassManager { contract_caches };
        #[cfg(feature = "cairo_native")]
        {
            if !config.run_cairo_native {
                // Native compilation is disabled - no need to start the compilation worker.
                return ContractClassManager { config, contract_caches, sender: None };
            }
            let (sender, receiver) = sync_channel(CHANNEL_SIZE);

            std::thread::spawn({
                let contract_caches = contract_caches.clone();
                let compiler_config = SierraToCasmCompilationConfig::default();
                let compiler = CommandLineCompiler::new(compiler_config);

                move || run_compilation_worker(contract_caches, receiver, compiler)
            });

            ContractClassManager { config, contract_caches, sender: Some(sender) }
        }
    }

    /// Sends a compilation request to the compilation worker. Does not block the sender. Logs an
    /// error if the channel is full.
    #[cfg(feature = "cairo_native")]
    pub fn send_compilation_request(&self, request: CompilationRequest) {
        assert!(self.config.run_cairo_native, "Native compilation is disabled.");
        let sender = self.sender.as_ref().expect("Compilation channel not available.");
        self.cache_request_contracts(&request);
        // TODO(Avi, 15/12/2024): Check for duplicated requests.
        sender.try_send(request).unwrap_or_else(|err| match err {
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
    #[cfg(feature = "cairo_native")]
    pub fn get_native(&self, class_hash: &ClassHash) -> Option<CachedCairoNative> {
        self.contract_caches.get_native(class_hash)
    }

    /// Returns the Sierra contract class for the given class hash, if it exists in cache.
    #[cfg(feature = "cairo_native")]
    pub fn get_sierra(&self, class_hash: &ClassHash) -> Option<Arc<SierraContractClass>> {
        self.contract_caches.get_sierra(class_hash)
    }

    /// Returns the casm compiled class for the given class hash, if it exists in cache.
    pub fn get_casm(&self, class_hash: &ClassHash) -> Option<RunnableCompiledClass> {
        self.contract_caches.get_casm(class_hash)
    }

    /// Sets the casm compiled class for the given class hash in the cache.
    pub fn set_casm(&self, class_hash: ClassHash, compiled_class: RunnableCompiledClass) {
        self.contract_caches.set_casm(class_hash, compiled_class);
    }

    #[cfg(feature = "cairo_native")]
    pub fn set_sierra(&self, class_hash: ClassHash, contract_class: Arc<SierraContractClass>) {
        self.contract_caches.set_sierra(class_hash, contract_class);
    }

    #[cfg(feature = "cairo_native")]
    pub fn run_cairo_native(&self) -> bool {
        self.config.run_cairo_native
    }

    #[cfg(feature = "cairo_native")]
    pub fn wait_on_native_compilation(&self) -> bool {
        self.config.wait_on_native_compilation
    }

    /// Clear the contract caches.
    pub fn clear(&mut self) {
        self.contract_caches.clear();
    }

    /// Caches the sierra and casm contract classes of a compilation request.
    #[cfg(feature = "cairo_native")]
    pub fn cache_request_contracts(&self, request: &CompilationRequest) {
        let (class_hash, sierra, casm) = request.clone();
        self.contract_caches.set_sierra(class_hash, sierra);
        self.contract_caches.set_casm(class_hash, RunnableCompiledClass::V1(casm));
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
    compiler: impl SierraToNativeCompiler,
) {
    info!("Compilation worker started.");
    for (class_hash, sierra, casm) in receiver.iter() {
        if contract_caches.get_native(&class_hash).is_some() {
            // The contract class is already compiled to native - skip the compilation.
            continue;
        }
        let sierra_for_compilation = into_contract_class_for_compilation(sierra.as_ref());
        let compilation_result = compiler.compile_to_native(sierra_for_compilation);
        match compilation_result {
            Ok(executor) => {
                let native_compiled_class = NativeCompiledClassV1::new(executor, casm);
                contract_caches
                    .set_native(class_hash, CachedCairoNative::Compiled(native_compiled_class));
            }
            Err(err) => {
                error!("Error compiling contract class: {}", err);
                contract_caches.set_native(class_hash, CachedCairoNative::CompilationFailed);
            }
        }
    }
    info!("Compilation worker terminated.");
}
