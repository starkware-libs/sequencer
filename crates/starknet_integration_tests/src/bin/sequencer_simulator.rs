use clap::Parser;
use starknet_integration_tests::integration_test_utils::set_panic_hook;
use starknet_integration_tests::sequencer_simulator_utils::SequencerSimulator;
use starknet_integration_tests::utils::{
    create_integration_test_tx_generator,
    BootstrapTxs,
    InvokeTxs,
    ACCOUNT_ID_0,
    N_TXS_IN_FIRST_BLOCK,
};
use starknet_sequencer_infra::trace_util::configure_tracing;
use tracing::info;
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    configure_tracing().await;
    set_panic_hook();
    const N_TXS: usize = 50;

    let args = Args::parse();

    let mut tx_generator = create_integration_test_tx_generator();

    let sequencer_simulator = SequencerSimulator::new(
        args.http_url,
        args.http_port,
        args.monitoring_url,
        args.monitoring_port,
    );

    info!("Sending bootstrap txs");
    sequencer_simulator.send_txs(&mut tx_generator, &BootstrapTxs, ACCOUNT_ID_0).await;

    // Wait for the bootstrap transaction to be accepted in a separate block.
    sequencer_simulator.await_txs_accepted(0, N_TXS_IN_FIRST_BLOCK).await;

    sequencer_simulator.send_txs(&mut tx_generator, &InvokeTxs(N_TXS), ACCOUNT_ID_0).await;

    sequencer_simulator.await_txs_accepted(0, N_TXS + N_TXS_IN_FIRST_BLOCK).await;

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

    #[arg(long)]
    http_port: u16,

    #[arg(long, default_value = "http://127.0.0.1")]
    monitoring_url: String,

    #[arg(long)]
    monitoring_port: u16,
}
