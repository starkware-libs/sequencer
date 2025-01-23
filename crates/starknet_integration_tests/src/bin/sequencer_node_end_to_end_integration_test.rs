use std::process::Command;

use starknet_integration_tests::end_to_end_integration::end_to_end_integration;
use starknet_integration_tests::utils::create_integration_test_tx_generator;
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

// When running `cargo build` - you get the error message:
// ERROR: ld.so: object '/usr/lib/x86_64-linux-gnu/libjemalloc.so' from LD_PRELOAD cannot be
// preloaded (cannot open shared object file): ignored.
//
// Copilot wrote the following comment:
// This is because the `jemalloc` library is not installed on the system. To fix this, you can
// install the library by running `sudo apt-get install libjemalloc2`.
//
// Time compare logs:
// End to end integration test took 147.841780323s
#[tokio::main]
async fn main() {
    configure_tracing().await;
    info!("Running integration test setup.");

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

    // Creates a multi-account transaction generator for integration test
    let mut tx_generator = create_integration_test_tx_generator();

    // Run end to end integration test.
    let start = std::time::Instant::now();
    end_to_end_integration(&mut tx_generator).await;
    tracing::error!("End to end integration test took {:?}", start.elapsed());
}
