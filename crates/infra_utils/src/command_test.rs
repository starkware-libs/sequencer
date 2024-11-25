use crate::command::create_shell_command;

#[test]
fn create_shell_command_example() {
    let mut ls_command = create_shell_command("ls");
    let output = ls_command.output().expect("Failed to execute command");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    // Project root should contain the `crates` directory.
    assert!(stdout.contains("crates"));
}
