use std::env::args;

use mempool_test_utils::starknet_api_test_utils::AccountId;
use starknet_api::block::BlockNumber;
use starknet_integration_tests::sequencer_simulator_utils::SequencerSimulator;
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
    const BLOCK_TO_WAIT_FOR_BOOTSTRAP: BlockNumber = BlockNumber(2);
    const N_TXS: usize = 50;

    let mut tx_generator = create_integration_test_tx_generator();

    let sequencer_simulator = SequencerSimulator::new(args().collect());

    info!("Sending bootstrap txs");
    sequencer_simulator.send_txs(&mut tx_generator, &BootstrapTxs, SENDER_ACCOUNT).await;

    sequencer_simulator.await_execution(BLOCK_TO_WAIT_FOR_BOOTSTRAP).await;

    sequencer_simulator.send_txs(&mut tx_generator, &InvokeTxs(N_TXS), SENDER_ACCOUNT).await;

    sequencer_simulator.await_execution(EXPECTED_BLOCK_NUMBER).await;

    // TODO(Nadin): pass node index as an argument.
    sequencer_simulator.verify_txs_accepted(0, &mut tx_generator, SENDER_ACCOUNT).await;

    info!("Simulation completed successfully");

    Ok(())
}
