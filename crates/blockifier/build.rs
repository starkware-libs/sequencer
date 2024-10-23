use std::path::PathBuf;
use std::process::Command;

fn compile_cairo_native_aot_runtime() {
    let cairo_native_dir = std::env::current_dir()
        .expect("Failed to get current directory")
        .join(PathBuf::from("cairo_native"));
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

    let runtime_expected_path = {
        let expected_path_env = std::env::var("CAIRO_NATIVE_RUNTIME_LIBRARY")
            .expect("'CAIRO_NATIVE_RUNTIME_LIBRARY' variable is not set");
        let expected_path = PathBuf::from(&expected_path_env);

        if expected_path.is_absolute() {
            expected_path
        } else {
            std::env::current_dir().expect("Failed to get current directory").join(expected_path)
        }
    };

    std::fs::copy(&runtime_target_path, &runtime_expected_path)
        .expect("Failed to copy native runtime");
}

fn main() {
    if std::env::var("CARGO_FEATURE_CAIRO_NATIVE").is_ok() {
        compile_cairo_native_aot_runtime();
    }
}
