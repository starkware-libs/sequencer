use std::path::PathBuf;
use std::process;

const TEST_FILE: &str = "./test_files/faulty_account.sierra.json";
const TEST_OUTPUT: &str = "./target/integration_test_output.so";

#[test]
fn test_running_the_crate_as_binary() {
    let binary_path = PathBuf::from(env!("CARGO_BIN_EXE_starknet-native-compile"));
    let output = process::Command::new(binary_path)
        .arg(TEST_FILE)
        .arg(TEST_OUTPUT)
        .output()
        .expect("Failed to run the binary");
    assert!(output.status.success());
    assert!(output.stderr.is_empty(), "{:?}", String::from_utf8_lossy(&output.stderr));
}
