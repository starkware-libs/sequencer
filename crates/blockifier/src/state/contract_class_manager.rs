use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::Arc;

use log::{error, info};
use starknet_api::core::ClassHash;
use starknet_api::state::SierraContractClass;
use starknet_sierra_compile::command_line_compiler::CommandLineCompiler;
use starknet_sierra_compile::config::SierraToCasmCompilationConfig;
use starknet_sierra_compile::utils::into_contract_class_for_compilation;
use starknet_sierra_compile::SierraToNativeCompiler;

use crate::blockifier::config::ContractClassManagerConfig;
use crate::execution::contract_class::{CompiledClassV1, RunnableCompiledClass};
use crate::execution::native::contract_class::NativeCompiledClassV1;
use crate::state::global_cache::{CachedCairoNative, ContractCaches};

const CHANNEL_SIZE: usize = 1000;

/// Represents a request to compile a sierra contract class to a native compiled class.
///
/// # Fields:
/// * `class_hash` - used to identify the contract class in the cache.
/// * `sierra_contract_class` - the sierra contract class to be compiled.
/// * `casm_compiled_class` - stored in [`NativeCompiledClassV1`] to allow fallback to cairo_vm
///   execution in case of unexpected failure during native execution.
type CompilationRequest = (ClassHash, Arc<SierraContractClass>, CompiledClassV1);

/// Manages the global cache of contract classes and handles sierra-to-native compilation requests.
struct ContractClassManager {
    config: ContractClassManagerConfig,
    /// The global cache of contract classes: casm, sierra, and native.
    contract_caches: Arc<ContractCaches>,
    /// The sending half of the compilation request channel. Set to `None` if native compilation is
    /// disabled.
    sender: Option<SyncSender<CompilationRequest>>,
}

#[allow(dead_code)]
impl ContractClassManager {
    /// Creates a new contract class manager and spawns a thread that listens for compilation
    /// requests and processes them (a.k.a. the compilation worker).
    /// Returns the contract class manager.
    pub fn start(config: ContractClassManagerConfig) -> ContractClassManager {
        // TODO(Avi, 15/12/2024): Add the size of the channel to the config.
        let contract_caches = Arc::new(ContractCaches::new(config.contract_cache_size));
        if !config.run_cairo_native {
            // Native compilation is disabled - no need to start the compilation worker.
            return ContractClassManager { config, contract_caches, sender: None };
        }
        let (sender, receiver) = sync_channel(CHANNEL_SIZE);
        let compiler_config = SierraToCasmCompilationConfig::default();
        let compiler = CommandLineCompiler::new(compiler_config);

        std::thread::spawn({
            let contract_caches = Arc::clone(&contract_caches);
            let compiler = Arc::new(compiler);

            move || run_compilation_worker(contract_caches, receiver, compiler)
        });

        ContractClassManager { config, contract_caches, sender: Some(sender) }
    }

    /// Sends a compilation request to the compilation worker. Does not block the sender. Logs an
    /// error if the channel is full.
    pub fn send_compilation_request(&self, request: CompilationRequest) {
        assert!(!self.config.run_cairo_native, "Native compilation is disabled.");
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
    pub fn get_native(&self, class_hash: &ClassHash) -> Option<CachedCairoNative> {
        self.contract_caches.get_native(class_hash)
    }

    /// Returns the Sierra contract class for the given class hash, if it exists in cache.
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

    /// Caches the sierra and casm contract classes of a compilation request.
    fn cache_request_contracts(&self, request: &CompilationRequest) {
        let (class_hash, sierra, casm) = request.clone();
        self.contract_caches.set_sierra(class_hash, sierra);
        let cached_casm = RunnableCompiledClass::from(casm);
        self.contract_caches.set_casm(class_hash, cached_casm);
    }
}

/// Handles compilation requests from the channel, holding the receiver end of the channel.
/// If no request is available, non-busy-waits until a request is available.
/// When the sender is dropped, the worker processes all pending requests and terminates.
fn run_compilation_worker(
    contract_caches: Arc<ContractCaches>,
    receiver: Receiver<CompilationRequest>,
    compiler: Arc<dyn SierraToNativeCompiler>,
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
                let native_compiled_class = NativeCompiledClassV1::new(executor.into(), casm);
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
