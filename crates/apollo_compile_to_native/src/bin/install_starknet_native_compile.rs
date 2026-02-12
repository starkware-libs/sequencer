use apollo_compilation_utils::build_utils::install_compiler_binary;
use apollo_compile_to_native::constants::{CAIRO_NATIVE_BINARY_NAME, REQUIRED_CAIRO_NATIVE_VERSION};

fn main() {
    install_compiler_binary(
        CAIRO_NATIVE_BINARY_NAME,
        REQUIRED_CAIRO_NATIVE_VERSION,
        &[CAIRO_NATIVE_BINARY_NAME, "--version", REQUIRED_CAIRO_NATIVE_VERSION],
    );
}
