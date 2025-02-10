use std::process::Command;

use starknet_api::block::BlockNumber;
use starknet_integration_tests::sequencer_manager::IntegrationTestManager;
use starknet_sequencer_infra::trace_util::configure_tracing;
use starknet_sequencer_node::test_utils::node_runner::get_node_executable_path;
use tracing::{error, info, warn};

// Logs the processes that are using the susceptible ports, for debugging purposes in case of port
// binding conflicts.
// TODO(Tsabary): Remove this function once the test is stable enough.
fn log_susceptible_ports() {
    const SUSCEPTIBLE_PORTS: [u16; 50] = [
        55000, 55001, 55002, 55060, 55061, 55062, 55120, 55121, 55122, 55180, 55181, 55182, 55240,
        55241, 55242, 55300, 55301, 55302, 55360, 55361, 55362, 55540, 55541, 55542, 55543, 55544,
        55545, 55546, 55547, 55548, 55549, 55550, 55551, 55552, 55553, 55554, 55555, 55556, 55557,
        55558, 55559, 55560, 55561, 55562, 55563, 55564, 55565, 55566, 55567, 55568,
    ];

    for &port in SUSCEPTIBLE_PORTS.iter() {
        let command = format!("lsof -i :{}", port);
        info!("Executing command: {}", command);

        // Execute the command.
        let output = Command::new("sh").arg("-c").arg(&command).output();

        match output {
            Ok(output) => {
                if output.stdout.is_empty() && output.stderr.is_empty() {
                    info!("Port {}: No output (command may not have found any result)", port);
                } else {
                    // Print the standard output and error.
                    info!(
                        "Port {}:\nSTDOUT:\n{}\nSTDERR:\n{}",
                        port,
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
            Err(e) => {
                error!("Failed to execute command for port {}: {}", port, e);
            }
        }
    }
}

#[tokio::main]
async fn main() {
    configure_tracing().await;
    info!("Running integration test setup.");

    set_ephemeral_port_range();

    // TODO(Tsabary): remove the hook definition once we transition to proper usage of task
    // spawning.
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log_susceptible_ports();
        default_panic(info);
        std::process::exit(1);
    }));

    let sequencer_path = get_node_executable_path();
    warn!(
        "This test uses a compiled sequencer node binary located at {}. Make sure to pre-compile \
         the binary before running this test. Alternatively, you can compile the binary and run \
         this test with './scripts/sequencer_integration_test.sh'",
        sequencer_path
    );

    // Run end to end integration test.
    end_to_end_positive_flow_integration().await;
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

pub async fn end_to_end_positive_flow_integration() {
    const BLOCK_TO_WAIT_FOR: BlockNumber = BlockNumber(15);
    const N_TXS: usize = 50;
    /// The number of consolidated local sequencers that participate in the test.
    const N_CONSOLIDATED_SEQUENCERS: usize = 3;
    /// The number of distributed remote sequencers that participate in the test.
    const N_DISTRIBUTED_SEQUENCERS: usize = 2;

    // Get the sequencer configurations.
    let mut integration_test_manager =
        IntegrationTestManager::new(N_CONSOLIDATED_SEQUENCERS, N_DISTRIBUTED_SEQUENCERS, None)
            .await;

    let node_indices = integration_test_manager.get_node_indices();
    // Run the nodes.
    integration_test_manager.run_nodes(node_indices.clone()).await;

    // Run the first block scenario to bootstrap the accounts.
    integration_test_manager.send_bootstrap_txs_and_verify().await;

    // Run the test.
    integration_test_manager.send_invoke_txs_and_verify(N_TXS, BLOCK_TO_WAIT_FOR).await;

    info!("Shutting down nodes.");
    integration_test_manager.shutdown_nodes(node_indices);

    info!("Positive flow integration test completed successfully!");
}
