use cairo_lang_starknet_classes::contract_class::ContractClass;
use cairo_native::executor::AotContractExecutor;
use starknet_compilation_utils::errors::CompilationUtilError;

pub mod command_line_compiler;
pub mod constants;

#[cfg(test)]
#[path = "compile_test.rs"]
pub mod compile_test;

#[cfg(test)]
#[path = "constants_test.rs"]
pub mod constants_test;

pub trait SierraToNativeCompiler: Send + Sync {
    fn compile(
        &self,
        contract_class: ContractClass,
    ) -> Result<AotContractExecutor, CompilationUtilError>;
}
