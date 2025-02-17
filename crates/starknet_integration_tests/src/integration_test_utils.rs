use std::process::Command;

use starknet_sequencer_infra::trace_util::configure_tracing;
use starknet_sequencer_node::test_utils::node_runner::get_node_executable_path;
use tracing::{info, warn};

// TODO(Tsabary): remove the hook definition once we transition to proper usage of task
// spawning.
pub fn set_panic_hook() {
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_panic(info);
        std::process::exit(1);
    }));
}

/// Adjusts the system's ephemeral port range to ensure predictable port allocation during tests.
///
/// By default, the operating system dynamically assigns ephemeral ports from a wide range,
/// which can lead to unpredictable port collisions in integration tests that rely on fixed port
/// usage. This function sets a narrower range (40000-40200) to limit port allocation to a small,
/// controlled set of ports, reducing the likelihood of conflicts.
fn set_ephemeral_port_range() {
    let output = Command::new("sudo")
        .arg("sysctl")
        .arg("-w")
        .arg("net.ipv4.ip_local_port_range=40000 40200")
        .output();

    match output {
        Ok(output) if output.status.success() => {
            info!("Ephemeral port range set successfully.");
        }
        Ok(output) => {
            eprintln!(
                "Failed to set ephemeral port range: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Err(e) => {
            eprintln!("Error executing sysctl command: {}", e);
        }
    }
}

pub async fn integration_test_setup(test_specifier: &str) {
    configure_tracing().await;
    info!("Running sequencer node end to end {test_specifier} flow integration test setup.");
    set_ephemeral_port_range();
    set_panic_hook();

    let sequencer_path = get_node_executable_path();
    warn!(
        "This test uses a compiled sequencer node binary located at {sequencer_path}. Make sure \
         to pre-compile the binary before running this test. Alternatively, you can compile the \
         binary and run this test with './scripts/sequencer_integration_test.sh {test_specifier}'"
    );
}
