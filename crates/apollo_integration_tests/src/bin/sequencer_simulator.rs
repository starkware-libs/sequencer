use std::fs::read_to_string;

use alloy::primitives::{Address as EthereumContractAddress, Address};
use apollo_infra::trace_util::configure_tracing;
use apollo_integration_tests::integration_test_manager::{HTTP_PORT_ARG, MONITORING_PORT_ARG};
use apollo_integration_tests::sequencer_simulator_utils::SequencerSimulator;
use apollo_integration_tests::utils::{
    create_integration_test_tx_generator,
    ConsensusTxs,
    DeployAndInvokeTxs,
    ACCOUNT_ID_0,
    N_TXS_IN_FIRST_BLOCK,
};
use clap::Parser;
use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use papyrus_base_layer::ethereum_base_layer_contract::EthereumBaseLayerConfig;
use papyrus_base_layer::test_utils::{
    deploy_starknet_l1_contract,
    make_block_history_on_anvil,
    DEFAULT_ANVIL_L1_DEPLOYED_ADDRESS,
};
use serde_json::Value;
use tokio::time::{sleep, Duration};
use tracing::info;
use url::Url;

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

async fn initialize_anvil_state(sender_address: Address, receiver_address: Address) {
    info!(
        "Initializing Anvil state with sender: {} and receiver: {}",
        sender_address, receiver_address
    );
    let starknet_contract_address: EthereumContractAddress =
        DEFAULT_ANVIL_L1_DEPLOYED_ADDRESS.parse().expect("Invalid address");
    let base_layer_config = EthereumBaseLayerConfig {
        node_url: Url::parse("http://localhost:8545").expect("Failed to parse Anvil URL"),
        starknet_contract_address,
        prague_blob_gas_calc: true,
        ..Default::default()
    };
    deploy_starknet_l1_contract(base_layer_config.clone()).await;

    let num_blocks_needed_on_l1 = 310;

    make_block_history_on_anvil(
        sender_address,
        receiver_address,
        base_layer_config.clone(),
        num_blocks_needed_on_l1,
    )
    .await;
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

    #[arg(long, help = "Anvil sender address (0x...)")]
    sender_address: String,

    #[arg(long, help = "Anvil receiver address (0x...)")]
    receiver_address: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    configure_tracing().await;

    let args = Args::parse();

    let sender_address = args.sender_address.parse::<Address>()?;
    let receiver_address = args.receiver_address.parse::<Address>()?;

    initialize_anvil_state(sender_address, receiver_address).await;

    let mut tx_generator = create_integration_test_tx_generator();

    let (http_port, monitoring_port) = get_ports(&args);

    let sequencer_simulator =
        SequencerSimulator::new(args.http_url, http_port, args.monitoring_url, monitoring_port);

    info!("Sending deploy and invoke txs");
    sequencer_simulator.send_txs(&mut tx_generator, &DeployAndInvokeTxs, ACCOUNT_ID_0).await;

    // Wait for the deploy and invoke transaction to be accepted in a separate block.
    sequencer_simulator.await_txs_accepted(0, N_TXS_IN_FIRST_BLOCK).await;

    run_simulation(&sequencer_simulator, &mut tx_generator, args.run_forever).await;

    // TODO(Nadin): pass node index as an argument.
    sequencer_simulator.verify_txs_accepted(0, &mut tx_generator, ACCOUNT_ID_0).await;

    info!("Simulation completed successfully");

    Ok(())
}
