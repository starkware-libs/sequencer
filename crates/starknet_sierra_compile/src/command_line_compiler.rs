use std::env;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_lang_starknet_classes::contract_class::ContractClass;
use tempfile::NamedTempFile;

use crate::config::SierraToCasmCompilationConfig;
use crate::errors::CompilationUtilError;
use crate::SierraToCasmCompiler;

#[derive(Clone)]
pub struct CommandLineCompiler {
    pub config: SierraToCasmCompilationConfig,
    path_to_starknet_sierra_compile_binary: &'static str,
}

impl CommandLineCompiler {
    pub fn new(config: SierraToCasmCompilationConfig) -> Self {
        Self { config, path_to_starknet_sierra_compile_binary: get_compiler_path() }
    }
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
        let temp_file_path = temp_file.path().to_str().ok_or(
            CompilationUtilError::CompilationError("Failed to get temporary file path".to_owned()),
        )?;

        // Set the parameters for the compile process.
        let mut command = Command::new(self.path_to_starknet_sierra_compile_binary);
        command.args([
            temp_file_path,
            "--add-pythonic-hints",
            "--max-bytecode-size",
            &self.config.max_bytecode_size.to_string(),
        ]);

        // Run the compile process.
        let compile_output = command.output()?;

        if !compile_output.status.success() {
            let stderr_output = String::from_utf8(compile_output.stderr)
                .unwrap_or("Failed to get stderr output".into());
            return Err(CompilationUtilError::CompilationError(stderr_output));
        };

        Ok(serde_json::from_slice::<CasmContractClass>(&compile_output.stdout)?)
    }
}

static COMPILER_PATH: OnceLock<&str> = OnceLock::new();
fn get_compiler_path() -> &'static str {
    COMPILER_PATH.get_or_init(|| {
        let out_dir = env::var("OUT_DIR").expect("Failed to get the OUT_DIR environment variable");
        let out_dir = Path::new(&out_dir);

        // Navigate up the directory tree until reaching the 'target' directory
        let target_dir = out_dir
            .ancestors()
            .nth(3)
            .expect("Failed to navigate up three levels from OUT_DIR")
            .join("cairo");

        let binary_name = "starknet-sierra-compile";
        Box::leak(
            target_dir
                .join("bin")
                .join(binary_name)
                .to_str()
                .expect("Failed to get compiler path")
                .into(),
        )
    })
}
