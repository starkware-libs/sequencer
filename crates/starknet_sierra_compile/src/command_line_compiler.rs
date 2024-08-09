use std::io::Write;
use std::process::Command;

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass;
use tempfile::NamedTempFile;

use crate::config::SierraToCasmCompilationConfig;
use crate::errors::CompilationUtilError;
use crate::SierraToCasmCompiler;

#[derive(Clone)]
pub struct CommandLineCompiler {
    pub config: SierraToCasmCompilationConfig,
}

impl SierraToCasmCompiler for CommandLineCompiler {
    fn compile(
        &self,
        contract_class: ContractClass,
    ) -> Result<CasmContractClass, CompilationUtilError> {
        // Create a temporary file to store the Sierra contract class.
        let serialized_contract_class = serde_json::to_string(&contract_class)?;

        let mut temp_file = NamedTempFile::new()?;
        temp_file.write_all(serialized_contract_class.as_bytes())?;
        let temp_file_path = temp_file.path().to_str().ok_or(CompilationUtilError::IoError(
            std::io::Error::new(std::io::ErrorKind::NotFound, "Failed to get temporary file path"),
        ))?;

        // Compile the Sierra contract class to Casm.
        let mut command = Command::new(excutable_file_location());
        command.arg(temp_file_path);

        command.arg("--add-pythonic-hints");
        command.args(["--max-bytecode-size", &self.config.max_bytecode_size.to_string()]);

        let compile_output = command.output()?;

        if !compile_output.status.success() {
            let stderr_output = String::from_utf8(compile_output.stderr)
                .unwrap_or("Failed to get stderr output".into());
            return Err(CompilationUtilError::CompilationError(stderr_output));
        };

        Ok(serde_json::from_slice::<CasmContractClass>(&compile_output.stdout)?)
    }
}

/// Returns the absolute path from the project root.
pub fn get_absolute_path(relative_path: &str) -> std::path::PathBuf {
    std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../..")
        .join(relative_path)
}

// TODO(Arni): Get the binary in a cleaner way.
fn excutable_file_location() -> String {
    const COMPILER_RELATIVE_PATH: &str =
        "crates/starknet_sierra_compile/executable/starknet-sierra-compile";
    get_absolute_path(COMPILER_RELATIVE_PATH).to_str().unwrap().to_owned()
}
