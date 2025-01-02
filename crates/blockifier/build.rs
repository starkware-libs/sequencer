#[cfg(feature = "cairo_native")]
fn compile_cairo_native_aot_runtime() {
    use std::path::PathBuf;
    use std::process::Command;

    use infra_utils::compile_time_cargo_manifest_dir;
    use infra_utils::path::current_dir;

    let cairo_native_dir =
        PathBuf::from(compile_time_cargo_manifest_dir!()).join(PathBuf::from("cairo_native"));

    if !cairo_native_dir.exists() || !cairo_native_dir.join(".git").exists() {
        panic!(
            "It seems git submodule at {} doesn't exist or it is not initialized, please \
             run:\n\ngit submodule update --init --recursive\n",
            cairo_native_dir.to_str().unwrap()
        );
    }
}

fn main() {
    // Build instructions are defined behind this condition since they are only relevant when using
    // Cairo Native.
    #[cfg(feature = "cairo_native")]
    compile_cairo_native_aot_runtime();
}
