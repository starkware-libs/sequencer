use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::Arc;
use std::time::Instant;

use apollo_compilation_utils::class_utils::into_contract_class_for_compilation;
use apollo_compilation_utils::errors::CompilationUtilError;
use apollo_compile_to_native::compiler::SierraToNativeCompiler;
#[cfg(any(feature = "testing", test))]
use cached::Cached;
use log;
use starknet_api::class_cache::GlobalContractCache;
use starknet_api::core::{ClassHash, CompiledClassHash};
use starknet_api::state::SierraContractClass;
use thiserror::Error;

use crate::blockifier::config::{
    CairoNativeRunConfig,
    ContractClassManagerConfig,
    NativeClassesWhitelist,
};
use crate::execution::contract_class::{CompiledClassV1, RunnableCompiledClass};
use crate::execution::native::contract_class::NativeCompiledClassV1;
use crate::metrics::NATIVE_COMPILATION_ERROR;
use crate::state::global_cache::{CachedCairoNative, CompiledClasses, RawClassCache};

#[cfg(test)]
#[path = "native_class_manager_test.rs"]
mod native_class_manager_test;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ContractClassManagerError {
    #[error("Error compiling contract class: {0}")]
    CompilationError(CompilationUtilError),
    #[error("Error when sending request: {0}")]
    TrySendError(TrySendError<ClassHash>),
}

/// Represents a request to compile a sierra contract class to a native compiled class.
///
/// # Fields:
/// * `class_hash` - used to identify the contract class in the class_cache.
/// * `sierra_contract_class` - the sierra contract class to be compiled.
/// * `casm_compiled_class` - stored in [`NativeCompiledClassV1`] to allow fallback to cairo_vm
///   execution in case of unexpected failure during native execution.
type CompilationRequest = (ClassHash, Arc<SierraContractClass>, CompiledClassV1);

/// Manages the global cache of contract classes and handles sierra-to-native compilation requests.
#[derive(Clone)]
pub struct NativeClassManager {
    cairo_native_run_config: CairoNativeRunConfig,
    /// The global cache of raw contract classes.
    class_cache: RawClassCache,
    /// The global cache of compiled class hashes v2.
    compiled_class_hash_v2_cache: GlobalContractCache<CompiledClassHash>,
    /// The sending half of the compilation request channel. Set to `None` if native compilation is
    /// disabled.
    sender: Option<SyncSender<CompilationRequest>>,
    /// The sierra-to-native compiler.
    compiler: Option<Arc<SierraToNativeCompiler>>,
}

impl NativeClassManager {
    /// Creates a new contract class manager and spawns a thread that listens for compilation
    /// requests and processes them (a.k.a. the compilation worker).
    /// Returns the contract class manager.
    /// NOTE: the compilation worker is not spawned if one of the following conditions is met:
    /// 1. The feature `cairo_native` is not enabled.
    /// 2. `config.run_cairo_native` is `false`.
    /// 3. `config.wait_on_native_compilation` is `true`.
    pub fn start(config: ContractClassManagerConfig) -> NativeClassManager {
        // TODO(Avi, 15/12/2024): Add the size of the channel to the config.
        let class_cache = RawClassCache::new(config.contract_cache_size);
        let compiled_class_hash_v2_cache = GlobalContractCache::new(config.contract_cache_size);
        let cairo_native_run_config = config.cairo_native_run_config;
        if !cairo_native_run_config.run_cairo_native {
            // Native compilation is disabled - no need to start the compilation worker.
            return NativeClassManager {
                cairo_native_run_config,
                class_cache,
                compiled_class_hash_v2_cache,
                sender: None,
                compiler: None,
            };
        }

        let compiler_config = config.native_compiler_config.clone();
        let compiler = Arc::new(SierraToNativeCompiler::new(compiler_config));
        if cairo_native_run_config.wait_on_native_compilation {
            // Compilation requests are processed synchronously. No need to start the worker.
            return NativeClassManager {
                cairo_native_run_config,
                class_cache,
                compiled_class_hash_v2_cache,
                sender: None,
                compiler: Some(compiler),
            };
        }

        let (sender, receiver) = sync_channel(cairo_native_run_config.channel_size);

        std::thread::spawn({
            let class_cache = class_cache.clone();
            move || {
                run_compilation_worker(
                    class_cache,
                    receiver,
                    compiler,
                    cairo_native_run_config.panic_on_compilation_failure,
                )
            }
        });

        // TODO(AVIV): Add private constructor with default values.
        NativeClassManager {
            cairo_native_run_config,
            class_cache,
            compiled_class_hash_v2_cache,
            sender: Some(sender),
            compiler: None,
        }
    }

    /// Returns the runnable compiled class for the given class hash, if it exists in class_cache.
    pub fn get_runnable(&self, class_hash: &ClassHash) -> Option<RunnableCompiledClass> {
        let cached_class = self.class_cache.get(class_hash)?;

        let cached_class = match cached_class {
            CompiledClasses::V1(_, _) => {
                // TODO(Yoni): make sure `wait_on_native_compilation` cannot be set to true while
                // `run_cairo_native` is false.
                assert!(
                    !self.wait_on_native_compilation(),
                    "Manager did not wait on native compilation."
                );
                cached_class
            }
            CompiledClasses::V1Native(CachedCairoNative::Compiled(native))
                if !self.run_class_with_cairo_native(class_hash) =>
            {
                CompiledClasses::V1(native.casm(), Arc::new(SierraContractClass::default()))
            }
            _ => cached_class,
        };

        Some(cached_class.to_runnable())
    }

    /// Caches the compiled class.
    /// For Cairo 1 classes:
    /// * if Native mode is enabled, triggers compilation to Native that will eventually be cached.
    /// * If `wait_on_native_compilation` is true, caches the Native variant immediately.
    pub fn set_and_compile(&self, class_hash: ClassHash, compiled_class: CompiledClasses) {
        match compiled_class {
            CompiledClasses::V0(_) => self.class_cache.set(class_hash, compiled_class),
            CompiledClasses::V1(compiled_class_v1, sierra_contract_class) => {
                // TODO(Yoni): instead of these two flag, use an enum.
                if self.wait_on_native_compilation() {
                    assert!(self.run_cairo_native(), "Native compilation is disabled.");
                    let compiler = self.compiler.as_ref().expect("Compiler not available.");
                    // After this point, the Native class should be cached and available through
                    // `get_runnable` access.
                    // Ignore compilation errors for now.
                    process_compilation_request(
                        self.class_cache.clone(),
                        compiler.clone(),
                        (class_hash, sierra_contract_class, compiled_class_v1),
                        self.cairo_native_run_config.panic_on_compilation_failure,
                    )
                    .unwrap_or(());
                    return;
                }

                // Cache the V1 class.
                self.class_cache.set(
                    class_hash,
                    CompiledClasses::V1(compiled_class_v1.clone(), sierra_contract_class.clone()),
                );
                if self.run_cairo_native() {
                    // Send a non-blocking compilation request.
                    // Ignore compilation errors for now.
                    self.send_compilation_request((
                        class_hash,
                        sierra_contract_class,
                        compiled_class_v1,
                    ))
                    .unwrap_or(());
                }
            }
            // TODO(Yoni): consider panic since this flow should not be reachable.
            CompiledClasses::V1Native(_) => self.class_cache.set(class_hash, compiled_class),
        }
    }

    /// Sends a compilation request to the compilation worker. Does not block the sender. Logs an
    /// error if the channel is full.
    fn send_compilation_request(
        &self,
        request: CompilationRequest,
    ) -> Result<(), ContractClassManagerError> {
        let sender = self.sender.as_ref().expect("Compilation channel not available.");
        // TODO(Avi, 15/12/2024): Check for duplicated requests.
        sender.try_send(request).map_err(|err| match err {
            TrySendError::Full((class_hash, _, _)) => {
                log::debug!(
                    "Compilation request channel is full (size: {}). Compilation request for \
                     class hash {} was not sent.",
                    self.cairo_native_run_config.channel_size,
                    class_hash
                );
                ContractClassManagerError::TrySendError(TrySendError::Full(class_hash))
            }
            TrySendError::Disconnected(_) => {
                panic!("Compilation request channel is closed.")
            }
        })
    }

    fn run_cairo_native(&self) -> bool {
        self.cairo_native_run_config.run_cairo_native
    }

    fn wait_on_native_compilation(&self) -> bool {
        self.cairo_native_run_config.wait_on_native_compilation
    }

    /// Determines if a contract should run with cairo native based on the whitelist.
    pub fn run_class_with_cairo_native(&self, class_hash: &ClassHash) -> bool {
        match &self.cairo_native_run_config.native_classes_whitelist {
            NativeClassesWhitelist::All => true,
            NativeClassesWhitelist::Limited(contracts) => contracts.contains(class_hash),
        }
    }

    /// Clears the contract class_cache.
    pub fn clear(&mut self) {
        self.class_cache.clear();
    }

    pub fn get_compiled_class_hash_v2(&self, class_hash: &ClassHash) -> Option<CompiledClassHash> {
        self.compiled_class_hash_v2_cache.get(class_hash)
    }

    pub fn set_compiled_class_hash_v2(
        &self,
        class_hash: ClassHash,
        compiled_class_hash_v2: CompiledClassHash,
    ) {
        self.compiled_class_hash_v2_cache.set(class_hash, compiled_class_hash_v2);
    }

    #[cfg(any(feature = "testing", test))]
    pub fn get_cache_size(&self) -> usize {
        self.class_cache.lock().cache_size()
    }
}

/// Handles compilation requests from the channel, holding the receiver end of the channel.
/// If no request is available, non-busy-waits until a request is available.
/// When the sender is dropped, the worker processes all pending requests and terminates.
fn run_compilation_worker(
    class_cache: RawClassCache,
    receiver: Receiver<CompilationRequest>,
    compiler: Arc<SierraToNativeCompiler>,
    panic_on_compilation_failure: bool,
) {
    log::info!("Compilation worker started.");
    for compilation_request in receiver.iter() {
        process_compilation_request(
            class_cache.clone(),
            compiler.clone(),
            compilation_request,
            panic_on_compilation_failure,
        )
        .unwrap_or(());
    }
    log::info!("Compilation worker terminated.");
}

/// Processes a compilation request and caches the result.
fn process_compilation_request(
    class_cache: RawClassCache,
    compiler: Arc<SierraToNativeCompiler>,
    compilation_request: CompilationRequest,
    panic_on_compilation_failure: bool,
) -> Result<(), CompilationUtilError> {
    let (class_hash, sierra, casm) = compilation_request;
    if let Some(CompiledClasses::V1Native(_)) = class_cache.get(&class_hash) {
        // The contract class is already compiled to native - skip the compilation.
        return Ok(());
    }
    let sierra_for_compilation = into_contract_class_for_compilation(sierra.as_ref());
    let start = Instant::now();
    let compilation_result = compiler.compile(sierra_for_compilation);
    let duration = start.elapsed();
    log::debug!(
        "Compiling to native contract with class hash: {}. Duration: {:.3} seconds",
        class_hash,
        duration.as_secs_f32()
    );
    match compilation_result {
        Ok(executor) => {
            let native_compiled_class = NativeCompiledClassV1::new(executor, casm);
            class_cache.set(
                class_hash,
                CompiledClasses::V1Native(CachedCairoNative::Compiled(native_compiled_class)),
            );
            log::debug!("Compilation succeeded");
            Ok(())
        }
        Err(err) => {
            class_cache.set(
                class_hash,
                CompiledClasses::V1Native(CachedCairoNative::CompilationFailed(casm)),
            );
            log::debug!("Error compiling contract class: {err}");
            NATIVE_COMPILATION_ERROR.increment(1);
            if panic_on_compilation_failure {
                panic!("Compilation failed");
            }
            Err(err)
        }
    }
}
