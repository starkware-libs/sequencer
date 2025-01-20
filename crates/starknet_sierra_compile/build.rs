fn main() {
    println!("cargo:rerun-if-changed=../../Cargo.lock");
    println!("cargo:rerun-if-changed=build.rs");

    set_run_time_out_dir_env_var();
}

// Sets the `RUNTIME_ACCESSIBLE_OUT_DIR` environment variable to the `OUT_DIR` value, which will be
// available only after the build is completed. Most importantly, it is available during runtime.
fn set_run_time_out_dir_env_var() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is not set");
    println!("cargo:rustc-env=RUNTIME_ACCESSIBLE_OUT_DIR={}", out_dir);
}
