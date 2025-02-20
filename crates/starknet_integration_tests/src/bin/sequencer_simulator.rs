use clap::Parser;
use starknet_api::block::BlockNumber;
use starknet_integration_tests::integration_test_utils::set_panic_hook;
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

    let sequencer_simulator = SequencerSimulator::new(args.config_file, args.url);

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
    #[arg(long)]
    config_file: String,

    #[arg(long, default_value = "http://127.0.0.1:8080")]
    url: String,
}
