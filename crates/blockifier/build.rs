use std::path::PathBuf;
use std::process::Command;

fn compile_cairo_native_aot_runtime() {
    let runtime_target_path = std::env::current_dir()
        .expect("Failed to get current directory")
        .join(PathBuf::from("./cairo_native/target/release/libcairo_native_runtime.a"));
    let runtime_target_dir = runtime_target_path.parent().unwrap().parent().unwrap();

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
        .expect("Failed to build cairo native runtime");
    if !status.success() {
        panic!("Building cairo native runtime failed!")
    }

    let runtime_target_path = std::env::current_dir()
        .expect("Failed to get current directory")
        .join(PathBuf::from("./cairo_native/target/release/libcairo_native_runtime.a"));

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

    let parent1 = runtime_target_path.parent().unwrap();
    let parent2 = parent1.parent().unwrap();
    let parent3 = parent2.parent().unwrap();
    let parent4 = parent3.parent().unwrap();
    let parent5 = parent4.parent().unwrap();

    let parent_info = format!(
        "
        parent1 {} (exists: {})
        parent2 {} (exists: {})
        parent3 {} (exists: {})
        parent4 {} (exists: {})
        parent5 {} (exists: {})
    
        ",
        parent1.to_str().unwrap(),
        parent1.exists(),
        parent2.to_str().unwrap(),
        parent2.exists(),
        parent3.to_str().unwrap(),
        parent3.exists(),
        parent4.to_str().unwrap(),
        parent4.exists(),
        parent5.to_str().unwrap(),
        parent5.exists(),
    );

    std::fs::copy(&runtime_target_path, &runtime_expected_path).unwrap_or_else(|err| {
        panic!(
            "Cannot copy cairo native runtime from \"{}\"(exists: {}) to \"{}\"(exists: \
             {}).Finally, parent info:\n{}. \nAnd err: {}",
            runtime_target_path.to_str().unwrap(),
            runtime_target_path.exists(),
            runtime_expected_path.to_str().unwrap(),
            runtime_expected_path.exists(),
            parent_info,
            err
        )
    });
}

fn main() {
    // todo(rdr): known issue of this approach is that if the user deletes libcairo_native_runtime.a
    // and re-compiles with further changes, `build.rs` won't activate
    compile_cairo_native_aot_runtime();
}
