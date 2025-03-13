use std::fs::read_to_string;

use clap::Parser;
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use serde_json::Value;
use starknet_integration_tests::integration_test_utils::set_panic_hook;
use starknet_integration_tests::sequencer_manager::{HTTP_PORT_ARG, MONITORING_PORT_ARG};
use starknet_integration_tests::sequencer_simulator_utils::SequencerSimulator;
use starknet_integration_tests::utils::{
    create_integration_test_tx_generator,
    BootstrapTxs,
    ConsensusTxs,
    ACCOUNT_ID_0,
    N_TXS_IN_FIRST_BLOCK,
};
use starknet_sequencer_infra::trace_util::configure_tracing;
use tokio::time::{sleep, Duration};
use tracing::info;

fn read_ports_from_file(path: &str) -> (u16, u16) {
    // Read the file content
    let file_content = read_to_string(path).unwrap();

    // Parse JSON
    let json: Value = serde_json::from_str(&file_content).unwrap();

    let http_port: u16 = json[HTTP_PORT_ARG]
        .as_u64()
        .unwrap_or_else(|| panic!("http port should be available in {}", path))
        .try_into()
        .expect("http port should be within the valid range for u16");

    let monitoring_port: u16 = json[MONITORING_PORT_ARG]
        .as_u64()
        .unwrap_or_else(|| panic!("monitoring port should be available in {}", path))
        .try_into()
        .expect("monitoring port should be within the valid range for u16");

    (http_port, monitoring_port)
}

fn get_ports(args: &Args) -> (u16, u16) {
    match (args.http_port, args.monitoring_port) {
        (Some(http), Some(monitoring)) => (http, monitoring),
        (None, None) => {
            if let Some(ref path) = args.simulator_ports_path {
                read_ports_from_file(path)
            } else {
                panic!(
                    "Either both --http-port and --monitoring-port should be supplied, or a \
                     --simulator-ports-path should be provided."
                );
            }
        }
        _ => panic!(
            "Either supply both --http-port and --monitoring-port, or use --simulator-ports-path."
        ),
    }
}

async fn run_simulation(
    sequencer_simulator: &SequencerSimulator,
    tx_generator: &mut MultiAccountTransactionGenerator,
    run_forever: bool,
) {
    const N_TXS: usize = 50;
    const SLEEP_DURATION: Duration = Duration::from_secs(1);

    let mut i = 1;
    loop {
        sequencer_simulator
            .send_txs(
                tx_generator,
                &ConsensusTxs {
                    n_invoke_txs: N_TXS,
                    // TODO(Arni): Add non-zero value.
                    n_l1_handler_txs: 0,
                },
                ACCOUNT_ID_0,
            )
            .await;
        sequencer_simulator.await_txs_accepted(0, i * N_TXS + N_TXS_IN_FIRST_BLOCK).await;

        if !run_forever {
            break;
        }

        sleep(SLEEP_DURATION).await;
        i += 1;
    }
}

#[derive(Parser, Debug)]
#[command(name = "sequencer_simulator", about = "Run sequencer simulator.")]
struct Args {
    #[arg(long, default_value = "http://127.0.0.1")]
    http_url: String,

    #[arg(long, default_value = "http://127.0.0.1")]
    monitoring_url: String,

    #[arg(long)]
    simulator_ports_path: Option<String>,

    #[arg(long)]
    http_port: Option<u16>,

    #[arg(long)]
    monitoring_port: Option<u16>,

    #[arg(long, help = "Run the simulator in an infinite loop")]
    run_forever: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    configure_tracing().await;
    set_panic_hook();

    let args = Args::parse();

    let mut tx_generator = create_integration_test_tx_generator();

    let (http_port, monitoring_port) = get_ports(&args);

    let sequencer_simulator =
        SequencerSimulator::new(args.http_url, http_port, args.monitoring_url, monitoring_port);

    info!("Sending bootstrap txs");
    sequencer_simulator.send_txs(&mut tx_generator, &BootstrapTxs, ACCOUNT_ID_0).await;

    // Wait for the bootstrap transaction to be accepted in a separate block.
    sequencer_simulator.await_txs_accepted(0, N_TXS_IN_FIRST_BLOCK).await;

    run_simulation(&sequencer_simulator, &mut tx_generator, args.run_forever).await;

    // TODO(Nadin): pass node index as an argument.
    sequencer_simulator.verify_txs_accepted(0, &mut tx_generator, ACCOUNT_ID_0).await;

    info!("Simulation completed successfully");

    Ok(())
}
