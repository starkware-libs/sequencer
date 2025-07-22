use apollo_compilation_utils::build_utils::install_compiler_binary;
use apollo_infra_utils::cairo_compiler_version::CAIRO1_COMPILER_VERSION;

include!("src/constants.rs");

fn main() {
    // Only rerun if build script or constants change - not for source code changes.
    println!("cargo:rerun-if-changed=build.rs");
    // FIXME: The constant `CAIRO1_COMPILER_VERSION` is not in this file, but the build script
    // should rerun if this constant changes. Should we add a copy of the constant in
    // `constants.rs`? Or should we add a `cargo:rerun-if-changed` for the external crate?
    println!("cargo:rerun-if-changed=src/constants.rs");

    set_run_time_out_dir_env_var();
    install_starknet_sierra_compile();
}

/// Installs the `starknet-sierra-compile` binary from the Cairo crate on StarkWare's release page
/// and moves it into `target` directory. The `starknet-sierra-compile` binary is used to compile
/// Sierra to Casm. The binary is executed as a subprocess whenever Sierra compilation is required.
fn install_starknet_sierra_compile() {
    let binary_name = CAIRO_LANG_BINARY_NAME;
    let required_version = CAIRO1_COMPILER_VERSION;

    let cargo_install_args = &[binary_name, "--version", required_version];
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
