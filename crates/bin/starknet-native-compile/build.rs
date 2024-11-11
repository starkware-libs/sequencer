use std::path::Path;

// This build script sets the env var CAIRO_NATIVE_RUNTIME_LIBRARY to an absolute path to the runtime library.
fn main() {
    println!("cargo:rerun-if-env-changed=CAIRO_NATIVE_RUNTIME_LIBRARY");
    if env!("CAIRO_NATIVE_RUNTIME_LIBRARY").starts_with('/') {
        // The runtime library path is already absolute, no need to set the env var.
        return;
    }
    let out_dir = Path::new(
        &std::env::var("OUT_DIR").expect("Failed to get the OUT_DIR environment variable"),
    )
    .to_path_buf();

    let blockifier_path = out_dir
        .ancestors()
        .nth(3)
        .expect("Failed to navigate up three levels from OUT_DIR")
        .to_path_buf();
    let runtime_library_abosulte_path = blockifier_path.join(env!("CAIRO_NATIVE_RUNTIME_LIBRARY"));
    std::env::set_var("CAIRO_NATIVE_RUNTIME_LIBRARY", runtime_library_abosulte_path);
}
