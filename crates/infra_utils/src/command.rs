use std::env;

use tokio::process::Command;

use papyrus_proc_macros::generate_get_package_dir;

generate_get_package_dir!();

#[cfg(test)]
#[path = "command_test.rs"]
mod command_test;

/// Returns a shell command originating from the project root, with cargo environment variables
/// filtered out.
///
/// # Arguments
/// * `command_name` - The shell command name.
///
/// # Returns
/// * A [`std::process::Command`] object with the current directory set to the project root, and
///   cleared out cargo related environment variables.
pub fn create_shell_command(command_name: &str) -> Command {
    let project_path = get_package_dir();
    let mut command = Command::new(command_name);
    command.current_dir(&project_path);
    // Filter out all CARGO_ environment variables.
    env::vars().filter(|(key, _)| key.starts_with("CARGO_")).for_each(|(key, _)| {
        command.env_remove(key);
    });
    command
}
