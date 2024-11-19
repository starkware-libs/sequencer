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

struct ContractClassManager {
    contract_class_caches: ContractClassCaches,
    sender: SyncSender<CompilationRequest>,
    receiver: Receiver<CompilationRequest>,
    compiler: CommandLineCompiler,
}

impl ContractClassManager {
    pub fn new(
        contract_class_caches: ContractClassCaches,
        sender: SyncSender<CompilationRequest>,
        receiver: Receiver<CompilationRequest>,
        compiler: CommandLineCompiler,
    ) -> Self {
        Self { contract_class_caches, sender, receiver, compiler }
    }

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

    pub fn try_send_compilation_request(
        &self,
        compilation_request: CompilationRequest,
    ) -> Result<(), TrySendError<CompilationRequest>> {
        Ok(self.sender.try_send(compilation_request)?)
    }
}
