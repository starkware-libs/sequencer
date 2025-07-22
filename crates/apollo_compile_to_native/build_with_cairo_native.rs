use apollo_compilation_utils::build_utils::install_compiler_binary;

include!("src/constants.rs");

fn main() {
    println!("cargo:rerun-if-changed=src/build_with_cairo_native.rs");

    set_run_time_out_dir_env_var();
    install_starknet_native_compile();
}

/// Install the `starknet-native-compile` binary from the Cairo Native crate and moves the binary
/// to the `target` directory. The `starknet-native-compile` binary is used to compile Sierra to
/// Native. The binary is executed as a subprocess whenever Sierra to Cairo compilation is required.
fn install_starknet_native_compile() {
    let binary_name = CAIRO_NATIVE_BINARY_NAME;
    let required_version = REQUIRED_CAIRO_NATIVE_VERSION;

    let cargo_install_args = &["cairo-native", "--version", required_version, "--bin", binary_name];
    install_compiler_binary(binary_name, required_version, cargo_install_args, &out_dir());
}

// Sets the `RUNTIME_ACCESSIBLE_OUT_DIR` environment variable to the `OUT_DIR` value, which will be
// available only after the build is completed. Most importantly, it is available during runtime.
fn set_run_time_out_dir_env_var() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is not set");
    println!("cargo:rustc-env=RUNTIME_ACCESSIBLE_OUT_DIR={}", out_dir);
}

// Returns the OUT_DIR. This function is only operable at build time.
fn out_dir() -> std::path::PathBuf {
    std::env::var("OUT_DIR")
        .expect("Failed to get the build time OUT_DIR environment variable")
        .into()
}
