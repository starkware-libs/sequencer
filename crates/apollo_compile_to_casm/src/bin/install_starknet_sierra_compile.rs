use apollo_compilation_utils::build_utils::install_compiler_binary;
use apollo_compile_to_casm::constants::CAIRO_LANG_BINARY_NAME;
use apollo_infra_utils::cairo_compiler_version::CAIRO1_COMPILER_VERSION;

fn main() {
    install_compiler_binary(
        CAIRO_LANG_BINARY_NAME,
        CAIRO1_COMPILER_VERSION,
        &[CAIRO_LANG_BINARY_NAME, "--version", CAIRO1_COMPILER_VERSION],
    );
}
