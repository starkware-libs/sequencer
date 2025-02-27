use std::fs::read_to_string;

use clap::Parser;
use serde_json::Value;
use starknet_api::block::BlockNumber;
use starknet_integration_tests::integration_test_utils::set_panic_hook;
use starknet_integration_tests::sequencer_manager::{HTTP_PORT_VAR, MONITORING_PORT_VAR};
use starknet_integration_tests::sequencer_simulator_utils::SequencerSimulator;
use starknet_integration_tests::utils::{
    create_integration_test_tx_generator,
    BootstrapTxs,
    InvokeTxs,
    ACCOUNT_ID_0,
};
use starknet_sequencer_infra::trace_util::configure_tracing;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    configure_tracing().await;
    set_panic_hook();
    const EXPECTED_BLOCK_NUMBER: BlockNumber = BlockNumber(10);
    const BLOCK_TO_WAIT_FOR_BOOTSTRAP: BlockNumber = BlockNumber(2);
    const N_TXS: usize = 50;

    let args = Args::parse();

    let mut tx_generator = create_integration_test_tx_generator();

    let (http_port, monitoring_port) = read_ports_from_file(&args.simulator_ports_path);

    let sequencer_simulator =
        SequencerSimulator::new(args.http_url, http_port, args.monitoring_url, monitoring_port);

    info!("Sending bootstrap txs");
    sequencer_simulator.send_txs(&mut tx_generator, &BootstrapTxs, ACCOUNT_ID_0).await;

    sequencer_simulator.await_execution(BLOCK_TO_WAIT_FOR_BOOTSTRAP).await;

    sequencer_simulator.send_txs(&mut tx_generator, &InvokeTxs(N_TXS), ACCOUNT_ID_0).await;

    sequencer_simulator.await_execution(EXPECTED_BLOCK_NUMBER).await;

    // TODO(Nadin): pass node index as an argument.
    sequencer_simulator.verify_txs_accepted(0, &mut tx_generator, ACCOUNT_ID_0).await;

    info!("Simulation completed successfully");

    Ok(())
}

#[derive(Parser, Debug)]
#[command(name = "sequencer_simulator", about = "Run sequencer simulator.")]
struct Args {
    #[arg(long, default_value = "http://127.0.0.1")]
    http_url: String,

    #[arg(long, default_value = "http://127.0.0.1")]
    monitoring_url: String,

    #[arg(long)]
    simulator_ports_path: String,
}

pub fn read_ports_from_file(path: &str) -> (u16, u16) {
    // Read the file content
    let file_content = read_to_string(path).unwrap();

    // Parse JSON
    let json: Value = serde_json::from_str(&file_content).unwrap();

    let http_port: u16 = json[HTTP_PORT_VAR]
        .as_u64()
        .expect(&format!("http port should be available in {}", path))
        .try_into()
        .expect("http port should be within the valid range for u16");

    let monitoring_port: u16 = json[MONITORING_PORT_VAR]
        .as_u64()
        .expect(&format!("monitoring port should be available in {}", path))
        .try_into()
        .expect("monitoring port should be within the valid range for u16");

    (http_port, monitoring_port)
}
