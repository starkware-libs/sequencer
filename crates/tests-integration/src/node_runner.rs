use std::env;
use std::process::{Command, Stdio};

const NODE_BINARY_NAME: &str = "target/debug/starknet_sequencer_node";

pub fn run_node(args: Vec<&str>) {
    // Get the current working directory for the project
    let project_path = env::current_dir().expect("Failed to get current directory");

    // Build path to the binary in the target/debug directory
    let mut binary_path = project_path;
    binary_path.push(NODE_BINARY_NAME);

    assert!(binary_path.exists(), "File does not exist: {:?}", &binary_path);

    // Run the compiled binary
    let run_status = Command::new(&binary_path)
        .args(args)
        .stderr(Stdio::inherit())
        .stdout(Stdio::inherit())
        .status()
        .expect("Failed to run compiled binary");

    assert!(run_status.success(), "Program finished unsuccessfully");
}
