use std::path::PathBuf;
use std::process::Command;

fn compile_cairo_native_aot_runtime() {
    let runtime_target_dir = std::env::current_dir()
        .expect("Failed to get current directory")
        .join(PathBuf::from("./cairo_native/target/"));

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
        .current_dir("cairo_native")
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
    // todo(rdr): known issue of this approach is that if the user deletes libcairo_native_runtime.a
    // and re-compiles with further changes, `build.rs` won't activate
    compile_cairo_native_aot_runtime();
}
