use std::env;
use std::env::temp_dir;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass;

use crate::config::SierraToCasmCompilationConfig;
use crate::errors::CompilationUtilError;
use crate::SierraToCasmCompiler;

// TODO(Arni): Solve Code duplication.
pub fn get_absolute_path(relative_path: &str) -> PathBuf {
    Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..").join(relative_path)
}

#[derive(Clone)]
pub struct CommandLineCompiler {
    pub config: SierraToCasmCompilationConfig,
}
impl SierraToCasmCompiler for CommandLineCompiler {
    fn compile_sierra_to_casm(
        &self,
        contract_class: ContractClass,
    ) -> Result<CasmContractClass, CompilationUtilError> {
        // TODO(Arni, 1/05/2024): Add the configurable parameters to the function.
        env::set_current_dir(get_absolute_path("")).expect("Failed to set current dir.");

        // Create a temporary file to store the Sierra contract class.
        let serialized_contract_class = serde_json::to_string(&contract_class).expect("number 1");

        let mut temp_path = temp_dir();
        temp_path.push("temp_file.sierra.json");
        let mut file = File::create(&temp_path).expect("number 2");
        file.write_all(serialized_contract_class.as_bytes()).expect("number 3");

        // Compile the Sierra contract class to Casm.
        let mut command = Command::new("starknet-sierra-compile");
        command.arg(temp_path.to_str().expect("number 4"));

        // Add pythonic hints should always be true.
        command.arg("--add-pythonic-hints");

        // TODO(Arni): use max-bytecode-size.
        command.arg("--max-bytecode-size");
        command.arg(self.config.max_bytecode_size.to_string());

        let compile_output =
            command.output().unwrap_or_else(|e| panic!("Failed to execute command: {}", e));

        if !compile_output.status.success() {
            let stderr_output = String::from_utf8(compile_output.stderr).expect("number 5"); // TODO: handle error
            return Err(CompilationUtilError::CompilationError(stderr_output));
        };

        Ok(serde_json::from_slice::<CasmContractClass>(&compile_output.stdout).expect("number 6"))
    }
}
