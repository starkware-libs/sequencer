use std::env;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use mempool_test_utils::starknet_api_test_utils::MultiAccountTransactionGenerator;
use papyrus_execution::execution_utils::get_nonce_at;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::StorageReader;
use rstest::{fixture, rstest};
use starknet_api::block::BlockNumber;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::state::StateNumber;
use starknet_integration_tests::integration_test_setup::IntegrationTestSetup;
use starknet_integration_tests::utils::{
    create_integration_test_tx_generator,
    run_transaction_generator_test_scenario,
};
use starknet_sequencer_infra::trace_util::configure_tracing;
use starknet_types_core::felt::Felt;
use tokio::process::{Child, Command};
use tokio::task::{self, JoinHandle};
use tokio::time::interval;
use tracing::{error, info};

#[fixture]
fn tx_generator() -> MultiAccountTransactionGenerator {
    create_integration_test_tx_generator()
}

// TODO(Tsabary): Move to a suitable util location.
async fn spawn_node_child_task(node_config_path: PathBuf) -> Child {
    // Get the current working directory for the project
    let project_path = env::current_dir().expect("Failed to get current directory").join("../..");

    // TODO(Tsabary): Capture output to a log file, and present it in case of a failure.
    // TODO(Tsabary): Change invocation from "cargo run" to separate compilation and invocation
    // (build, and then invoke the binary).
    Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("starknet_sequencer_node")
        .arg("--quiet")
        .current_dir(&project_path)
        .arg("--")
        .arg("--config_file")
        .arg(node_config_path.to_str().unwrap())
        .stderr(Stdio::inherit())
        .stdout(Stdio::null())
        .kill_on_drop(true) // Required for stopping the node when the handle is dropped.
        .spawn()
        .expect("Failed to spawn the sequencer node.")
}

async fn spawn_run_node(node_config_path: PathBuf) -> JoinHandle<()> {
    task::spawn(async move {
        info!("Running the node from its spawned task.");
        let _node_run_result = spawn_node_child_task(node_config_path).
            await. // awaits the completion of spawn_node_child_task.
            wait(). // runs the node until completion -- should be running indefinitely.
            await; // awaits the completion of the node.
        panic!("Node stopped unexpectedly.");
    })
}

/// Reads the latest block number from the storage.
fn get_latest_block_number(storage_reader: &StorageReader) -> BlockNumber {
    let txn = storage_reader.begin_ro_txn().unwrap();
    txn.get_state_marker()
        .expect("There should always be a state marker")
        .prev()
        .expect("There should be a previous block in the storage, set by the test setup")
}

/// Reads an account nonce after a block number from storage.
fn get_account_nonce(
    storage_reader: &StorageReader,
    block_number: BlockNumber,
    contract_address: ContractAddress,
) -> Nonce {
    let txn = storage_reader.begin_ro_txn().unwrap();
    let state_number = StateNumber::unchecked_right_after_block(block_number);
    get_nonce_at(&txn, state_number, None, contract_address)
        .expect("Should always be Ok(Some(Nonce))")
        .expect("Should always be Some(Nonce)")
}

/// Sample a storage until sufficiently many blocks have been stored. Returns an error if after
/// the given number of attempts the target block number has not been reached.
async fn await_block(
    interval_duration: Duration,
    target_block_number: BlockNumber,
    max_attempts: usize,
    storage_reader: &StorageReader,
) -> Result<(), ()> {
    let mut interval = interval(interval_duration);
    let mut count = 0;
    loop {
        // Read the latest block number.
        let latest_block_number = get_latest_block_number(storage_reader);
        count += 1;

        // Check if reached the target block number.
        if latest_block_number >= target_block_number {
            info!("Found block {} after {} queries.", target_block_number, count);
            return Ok(());
        }

        // Check if reached the maximum attempts.
        if count > max_attempts {
            error!(
                "Latest block is {}, expected {}, stopping sampling.",
                latest_block_number, target_block_number
            );
            return Err(());
        }

        // Wait for the next interval.
        interval.tick().await;
    }
}

#[rstest]
#[tokio::test]
async fn test_end_to_end_integration(tx_generator: MultiAccountTransactionGenerator) {
    const EXPECTED_BLOCK_NUMBER: BlockNumber = BlockNumber(15);

    configure_tracing();
    info!("Running integration test setup.");

    // Creating the storage for the test.

    let integration_test_setup = IntegrationTestSetup::new_from_tx_generator(&tx_generator).await;

    info!("Running sequencer node.");
    let node_run_handle = spawn_run_node(integration_test_setup.node_config_path).await;

    // Wait for the node to start.
    match integration_test_setup.is_alive_test_client.await_alive(Duration::from_secs(5), 30).await
    {
        Ok(_) => {}
        Err(_) => panic!("Node is not alive."),
    }

    info!("Running integration test simulator.");

    let send_rpc_tx_fn =
        &mut |rpc_tx| integration_test_setup.add_tx_http_client.assert_add_tx_success(rpc_tx);

    let n_txs = 50;
    info!("Sending {n_txs} txs.");
    let (tx_hashes, sender_address) =
        run_transaction_generator_test_scenario(tx_generator, n_txs, send_rpc_tx_fn).await;

    info!("Awaiting until {EXPECTED_BLOCK_NUMBER} blocks have been created.");

    let (batcher_storage_reader, _) =
        papyrus_storage::open_storage(integration_test_setup.batcher_storage_config)
            .expect("Failed to open batcher's storage");

    match await_block(Duration::from_secs(5), EXPECTED_BLOCK_NUMBER, 15, &batcher_storage_reader)
        .await
    {
        Ok(_) => {}
        Err(_) => panic!("Did not reach expected block number."),
    }

    info!("Shutting the node down.");
    node_run_handle.abort();
    let res = node_run_handle.await;
    assert!(
        res.expect_err("Node should have been stopped.").is_cancelled(),
        "Node should have been stopped."
    );

    info!("Verifying tx sender account nonce.");
    let expected_nonce_value = tx_hashes.len() + 1;
    let expected_nonce =
        Nonce(Felt::from_hex_unchecked(format!("0x{:X}", expected_nonce_value).as_str()));
    let nonce = get_account_nonce(&batcher_storage_reader, EXPECTED_BLOCK_NUMBER, sender_address);
    assert_eq!(nonce, expected_nonce);
}
