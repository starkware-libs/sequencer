use apollo_compile_to_casm::compiler::SierraToCasmCompiler;
use apollo_sierra_compilation_config::config::SierraCompilationConfig;

fn main() {
    let _compiler = SierraToCasmCompiler::new(SierraCompilationConfig::default());
}
