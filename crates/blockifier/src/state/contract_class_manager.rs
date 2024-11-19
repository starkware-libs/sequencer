use std::sync::mpsc::{Receiver, SyncSender, TrySendError};
use std::sync::Arc;

use log::error;
use starknet_api::core::ClassHash;
use starknet_api::state::ContractClass as SierraContractClass;
use starknet_sierra_compile::command_line_compiler::CommandLineCompiler;
use starknet_sierra_compile::SierraToNativeCompiler;

use crate::execution::contract_class::ContractClassV1;
use crate::execution::native::contract_class::NativeContractClassV1;
use crate::state::global_cache::{CachedCairoNative, ContractClassCaches};

type CompilationRequest = (ClassHash, Arc<SierraContractClass>, ContractClassV1);

/// Manages the global cache of contract classes and handles compilation requests from sierra to
/// native. Used by both request sender and receiver threads. Sender threads send compilation
/// requests to the manager, and receiver thread sequentially process them - compiling the sierra
/// contract class in the request to a native contact class and storing the result in the cache.
struct ContractClassManager {
    contract_class_caches: ContractClassCaches,
    sender: SyncSender<CompilationRequest>,
    receiver: Receiver<CompilationRequest>,
    compiler: CommandLineCompiler,
}

impl ContractClassManager {
    pub fn new(
        // The global cache of contract classes: casm, sierra, and native.
        contract_class_caches: ContractClassCaches,
        // The sending half of the channel for sending compilation requests.
        sender: SyncSender<CompilationRequest>,
        // The receiving half of the channel for receiving compilation requests.
        receiver: Receiver<CompilationRequest>,
        // The compiler used to compile sierra contract classes to native contract classes.
        compiler: CommandLineCompiler,
    ) -> Self {
        Self { contract_class_caches, sender, receiver, compiler }
    }

    /// Handles compilation requests from the receiver. For each request, compiles the sierra
    /// contract class to a native contract class and stores the result in the cache.
    /// If no request is available, non-busy-waits until a request is available.
    pub fn handle_compilation_requests_loop(&self) {
        for (class_hash, sierra, casm) in self.receiver.iter() {
            if self.contract_class_caches.get_native(&class_hash).is_some() {
                // The contract class is already compiled to native - skip the compilation.
                continue;
            }
            // TODO(Avi): Convert `sierra_contract_class` to
            // `cairo_lang_starknet_classes::contract_class::ContractClass`
            let compilation_result = self.compiler.compile_to_native(sierra.into());
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

    /// Tries to send a compilation request to the manager. Does not block the sender thread. If
    /// the channel is full, returns an error.
    pub fn try_send_compilation_request(
        &self,
        compilation_request: CompilationRequest,
    ) -> Result<(), TrySendError<CompilationRequest>> {
        Ok(self.sender.try_send(compilation_request)?)
    }
}
