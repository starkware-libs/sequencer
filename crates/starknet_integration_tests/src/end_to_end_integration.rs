use std::collections::HashSet;
use std::time::Duration;

use mempool_test_utils::starknet_api_test_utils::{AccountId, MultiAccountTransactionGenerator};
use starknet_api::block::BlockNumber;
use starknet_sequencer_node::test_utils::node_runner::get_node_executable_path;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::sleep;
use tracing::{error, info};

use crate::sequencer_manager::{get_sequencer_setup_configs, IntegrationTestManager};
use crate::utils::{BootstrapTxs, InvokeTxs};

async fn obtain_port() {
    // Run the `nc -l -p 55550` command
    let mut child = match Command::new("nc")
        .args(["-l", "-p", "55550"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => {
            info!("Started `nc -l -p 55550` successfully.");
            child
        }
        Err(e) => {
            error!("Failed to start `nc`: {}", e);
            return;
        }
    };

    // Read stdout and stderr asynchronously
    if let Some(stdout) = child.stdout.take() {
        let mut reader = BufReader::new(stdout).lines();

        tokio::spawn(async move {
            while let Ok(Some(line)) = reader.next_line().await {
                info!("nc output: {}", line);
            }
        });
    }

    if let Some(stderr) = child.stderr.take() {
        let mut reader = BufReader::new(stderr).lines();

        tokio::spawn(async move {
            while let Ok(Some(line)) = reader.next_line().await {
                error!("nc error: {}", line);
            }
        });
    }

    // Keep the program running so the `nc` process stays active
    sleep(Duration::from_secs(30)).await;
    info!("Shutting down `nc` after 30 seconds.");

    // Kill the `nc` process when done
    if let Err(e) = child.kill().await {
        error!("Failed to kill `nc`: {}", e);
    }
}

// async fn wait_some_time() {
//     info!("Sleeping for 30 seconds...");
//     sleep(Duration::from_secs(30)).await;
//     info!("Woke up after 30 seconds!");
// }

pub async fn end_to_end_integration(tx_generator: &mut MultiAccountTransactionGenerator) {
    const BLOCK_TO_WAIT_FOR_BOOTSTRAP: BlockNumber = BlockNumber(2);
    const BLOCK_TO_WAIT_FOR_FIRST_ROUND: BlockNumber = BlockNumber(10);
    const BLOCK_TO_WAIT_FOR_LATE_NODE: BlockNumber = BlockNumber(25);
    const N_TXS: usize = 50;
    const SENDER_ACCOUNT: AccountId = 0;
    /// The number of consolidated local sequencers that participate in the test.
    const N_CONSOLIDATED_SEQUENCERS: usize = 3;
    /// The number of distributed remote sequencers that participate in the test.
    const N_DISTRIBUTED_SEQUENCERS: usize = 2;

    info!("Checking that the sequencer node executable is present.");
    get_node_executable_path();

    obtain_port().await;

    // Get the sequencer configurations.
    let (sequencers_setup, node_indices) = get_sequencer_setup_configs(
        tx_generator,
        N_CONSOLIDATED_SEQUENCERS,
        N_DISTRIBUTED_SEQUENCERS,
    )
    .await;

    // Run the sequencers.
    // TODO(Nadin, Tsabary): Refactor to separate the construction of SequencerManager from its
    // invocation. Consider using the builder pattern.
    let mut integration_test_manager = IntegrationTestManager::new(sequencers_setup, Vec::new());

    // Remove the node with index 1 to simulate a late node.
    let mut filtered_nodes = node_indices.clone();
    filtered_nodes.remove(&1);

    // Run the nodes.
    integration_test_manager.run(filtered_nodes).await;

    // Run the first block scenario to bootstrap the accounts.
    integration_test_manager
        .test_and_verify(tx_generator, BootstrapTxs, SENDER_ACCOUNT, BLOCK_TO_WAIT_FOR_BOOTSTRAP)
        .await;

    // Run the test.
    integration_test_manager
        .test_and_verify(
            tx_generator,
            InvokeTxs(N_TXS),
            SENDER_ACCOUNT,
            BLOCK_TO_WAIT_FOR_FIRST_ROUND,
        )
        .await;

    // Run the late node.
    integration_test_manager.run(HashSet::from([1])).await;

    // Run the tests after the late node joins.
    integration_test_manager
        .test_and_verify(
            tx_generator,
            InvokeTxs(N_TXS),
            SENDER_ACCOUNT,
            BLOCK_TO_WAIT_FOR_LATE_NODE,
        )
        .await;

    info!("Shutting down nodes.");
    integration_test_manager.shutdown_nodes(node_indices);

    info!("Integration test completed successfully!");
    panic!("This should panic.");
}
