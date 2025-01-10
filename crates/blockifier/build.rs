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

    let runtime_target_dir = cairo_native_dir.join(PathBuf::from("target"));
    let status = Command::new("cargo")
        .args([
            "build",
            "--release",
            "-p",
            "cairo-native-runtime",
            "--message-format=json",
            "--target-dir",
            runtime_target_dir.to_str().unwrap(),
        ])
        .current_dir(cairo_native_dir)
        .status()
        .expect("Failed to execute cargo");
    if !status.success() {
        panic!("Building cairo native runtime failed: {status}")
    }

    let runtime_target_path =
        runtime_target_dir.join(PathBuf::from("release/libcairo_native_runtime.a"));

    const RUNTIME_LIBRARY: &str = "CAIRO_NATIVE_RUNTIME_LIBRARY";
    let runtime_expected_path = {
        let expected_path_env =
            std::env::var(RUNTIME_LIBRARY).expect("Cairo Native runtime path variable is not set");
        let expected_path = PathBuf::from(&expected_path_env);

        if expected_path.is_absolute() {
            expected_path
        } else {
            current_dir().expect("Failed to get current directory").join(expected_path)
        }
    };

    std::fs::copy(&runtime_target_path, &runtime_expected_path)
        .expect("Failed to copy native runtime");

    println!("cargo::rerun-if-changed=./cairo_native/runtime/");
    // todo(rodrigo): this directive seems to cause the build script to trigger everytime on
    // Linux based machines. Investigate the issue further.
    println!("cargo::rerun-if-changed={}", runtime_expected_path.to_str().unwrap());
    println!("cargo::rerun-if-env-changed={RUNTIME_LIBRARY}");
}

fn main() {
    // Build instructions are defined behind this condition since they are only relevant when using
    // Cairo Native.
    #[cfg(feature = "cairo_native")]
    compile_cairo_native_aot_runtime();
}
