use std::env::args;
use std::net::SocketAddr;
use std::process::exit;

use mempool_test_utils::starknet_api_test_utils::AccountId;
use starknet_api::block::BlockNumber;
use starknet_integration_tests::sequencer_simulator::SequencerSimulator;
use starknet_integration_tests::utils::{
    create_integration_test_tx_generator,
    BootstrapTxs,
    InvokeTxs,
};
use starknet_sequencer_infra::trace_util::configure_tracing;
use tracing::info;
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    configure_tracing().await;

    // TODO(Tsabary): remove the hook definition once we transition to proper usage of task
    // spawning.
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_panic(info);
        std::process::exit(1);
    }));
    const SENDER_ACCOUNT: AccountId = 0;
    const EXPECTED_BLOCK_NUMBER: BlockNumber = BlockNumber(10);
    const N_TXS: usize = 50;

    // Creates a multi-account transaction generator for integration test
    let mut tx_generator = create_integration_test_tx_generator();

    info!("After tx genrator");

    let sequencer_simulator = SequencerSimulator::new(args().collect());

    info!("After sequencer_simulator");

    info!("Bootstrapping sequencer simulator.");
    let bootstrap_scenario = BootstrapTxs;

    info!("Sending bootstrap txs");
    sequencer_simulator.send_txs(&mut tx_generator, &bootstrap_scenario, SENDER_ACCOUNT).await;

    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // // let (sequencers_setup, _node_indices) = get_sequencer_setup_configs(tx_generator, 1,
    // 0).await;

    // info!("After tx genrator");

    let test_scenario = InvokeTxs(N_TXS);

    sequencer_simulator.send_txs(&mut tx_generator, &test_scenario, SENDER_ACCOUNT).await;

    info!("After send_txs");

    // sequencer_simulator.await_execution(EXPECTED_BLOCK_NUMBER).await;

    // info!("System test simulator finished.");
    Ok(())
}
