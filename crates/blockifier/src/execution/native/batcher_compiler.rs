use std::sync::Arc;

use crossbeam_channel::Receiver;
use starknet_api::core::ClassHash;
use starknet_api::state::ContractClass as SierraContractClass;
use starknet_sierra_compile::command_line_compiler::CommandLineCompiler;
use starknet_sierra_compile::SierraToNativeCompiler;

use crate::execution::native::contract_class::NativeContractClassV1;
use crate::state::global_cache::GlobalContractCacheManager;

type CompilationRequest = (ClassHash, Arc<SierraContractClass>);

struct BatcherCompiler {
    contract_cache_manager: GlobalContractCacheManager,
    compilation_request_receiver: Receiver<CompilationRequest>,
    command_line_compiler: CommandLineCompiler,
}

impl BatcherCompiler {
    pub fn new(
        contract_cache_manager: GlobalContractCacheManager,
        compilation_request_receiver: Receiver<CompilationRequest>,
        command_line_compiler: CommandLineCompiler,
    ) -> Self {
        Self { contract_cache_manager, compilation_request_receiver, command_line_compiler }
    }
    pub fn run(&self) {
        for (class_hash, sierra_contract_class) in self.compilation_request_receiver.iter() {
            if self.contract_cache_manager.get_native_contract_executor(&class_hash).is_some() {
                // Skip the compilation if the contract class is already compiled to native.
                continue;
            }
            // TODO(Avi): Convert `sierra_contract_class` to
            // `cairo_lang_starknet_classes::contract_class::ContractClass`
            let compilation_result =
                self.command_line_compiler.compile_to_native(sierra_contract_class.into());
            match compilation_result {
                Ok(contract_executor) => {
                    self.contract_cache_manager
                        .set_native_contract_executor(class_hash, Some(contract_executor));
                }
                Err(err) => {
                    eprintln!("Error compiling contract class: {}", err);
                    self.contract_cache_manager.set_native_contract_executor(class_hash, None);
                }
            }
        }
    }
}
