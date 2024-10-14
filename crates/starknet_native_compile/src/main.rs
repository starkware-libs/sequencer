use std::process::{self, Command};

#[cfg(test)]
#[path = "test_build.rs"]
mod test_build;

mod build_utils;

fn main() {
    println!("Parent PID: {}", process::id());

    // FIXME: Using `cargo install` makes the binary available in the PATH. Should we make it a
    // separate crate?
    // Instructions: In `build.rs`, we should run cargo install
    // - start with a version check (if the binary is already installed)
    // - for the versions check to work, we need to add an `--version` argument to the binary.
    // - specify `--path` in `cargo install` as the root of the current crate.
    // - `--bin` should be given the name of the binary to install.
    // - `--root` should point to the installation folder. (we shouldn't use the default.)
    let exe_path = "native-contract-compile";
    // let exe_path = std::env::var("CARGO_BIN_EXE_native-contract-compile")
    // .expect("Environment variable CARGO_BIN_EXE_native-contract-compile not defined");
    // let exe_path = PathBuf::from("./native-contract-compile");
    // let exe_path = std::env::current_exe()
    //     .expect("Failed to get current executable path")
    //     .with_file_name("native-contract-compile");
    println!("Executable path: {:?}", exe_path);

    let mut child = Command::new(exe_path).spawn().expect("Failed to spawn child process");

    println!("Spawned child PID: {}", child.id());

    // Optionally, perform other tasks here

    let status = child.wait().expect("Failed to wait on child");

    println!("Child exited with status: {}", status);
}
