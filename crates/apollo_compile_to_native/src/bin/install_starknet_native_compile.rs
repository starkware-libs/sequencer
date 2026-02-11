use apollo_compile_to_native::compiler::SierraToNativeCompiler;
use apollo_compile_to_native_types::SierraCompilationConfig;

fn main() {
    let _compiler = SierraToNativeCompiler::new(SierraCompilationConfig::default());
}
