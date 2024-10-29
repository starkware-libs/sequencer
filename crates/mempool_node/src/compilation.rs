use std::process::{Command, ExitStatus, Stdio};
use std::{env, io};

use tracing::info;

#[cfg(test)]
#[path = "compilation_test.rs"]
mod compilation_test;

fn compile_node() -> io::Result<ExitStatus> {
    info!("Compiling the project");
    // Get the current working directory for the project
    let project_path = env::current_dir().expect("Failed to get current directory");

    // Run `cargo build` to compile the project
    let compilation_result = Command::new("cargo")
        .arg("build")
        .current_dir(&project_path)
        .stderr(Stdio::inherit())
        .stdout(Stdio::inherit())
        .status();

    info!("Compilation result: {:?}", compilation_result);
    compilation_result
}

pub fn compile_node_with_status() -> bool {
    compile_node().is_ok_and(|x| x.success())
}
