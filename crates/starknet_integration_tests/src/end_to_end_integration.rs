use infra_utils::run_until::run_until;
use infra_utils::tracing::{CustomLogger, TraceLevel};
use mempool_test_utils::starknet_api_test_utils::{AccountId, MultiAccountTransactionGenerator};
use papyrus_execution::execution_utils::get_nonce_at;
use papyrus_storage::state::StateStorageReader;
use papyrus_storage::StorageReader;
use starknet_api::block::BlockNumber;
use starknet_api::core::{ContractAddress, Nonce};
use starknet_api::state::StateNumber;
use starknet_sequencer_node::test_utils::node_runner::{get_node_executable_path, spawn_run_node};
use starknet_types_core::felt::Felt;
use tokio::join;
use tracing::info;

use crate::integration_test_setup::IntegrationTestSetup;
use crate::utils::send_account_txs;

/// Reads the latest block number from the storage.
fn get_latest_block_number(storage_reader: &StorageReader) -> BlockNumber {
    let txn = storage_reader.begin_ro_txn().unwrap();
    txn.get_state_marker()
        .expect("There should always be a state marker")
        .prev()
        .expect("There should be a previous block in the storage, set by the test setup")
}

/// Reads an account nonce after a block number from storage.
fn get_account_nonce(storage_reader: &StorageReader, contract_address: ContractAddress) -> Nonce {
    let block_number = get_latest_block_number(storage_reader);
    let txn = storage_reader.begin_ro_txn().unwrap();
    let state_number = StateNumber::unchecked_right_after_block(block_number);
    get_nonce_at(&txn, state_number, None, contract_address)
        .expect("Should always be Ok(Some(Nonce))")
        .expect("Should always be Some(Nonce)")
}

/// Sample a storage until sufficiently many blocks have been stored. Returns an error if after
/// the given number of attempts the target block number has not been reached.
async fn await_block(
    interval: u64,
    target_block_number: BlockNumber,
    max_attempts: usize,
    storage_reader: &StorageReader,
) -> Result<BlockNumber, ()> {
    let condition = |&latest_block_number: &BlockNumber| latest_block_number >= target_block_number;
    let get_latest_block_number_closure = || async move { get_latest_block_number(storage_reader) };

    let logger = CustomLogger::new(
        TraceLevel::Info,
        Some("Waiting for storage to include block".to_string()),
    );

    run_until(interval, max_attempts, get_latest_block_number_closure, condition, Some(logger))
        .await
        .ok_or(())
}

pub async fn end_to_end_integration(mut tx_generator: MultiAccountTransactionGenerator) {
    const EXPECTED_BLOCK_NUMBER: BlockNumber = BlockNumber(15);

    info!("Checking that the sequencer node executable is present.");
    get_node_executable_path();

    info!("Running integration test setup.");
    // Creating the storage for the test.
    let integration_test_setup = IntegrationTestSetup::new_from_tx_generator(&tx_generator).await;

    let node_0_run_handle =
        spawn_run_node(integration_test_setup.sequencer_0.node_config_path.clone());
    let node_1_run_handle =
        spawn_run_node(integration_test_setup.sequencer_1.node_config_path.clone());

    info!("Running sequencers.");
    let (node_0_run_task, node_1_run_task) = join!(node_0_run_handle, node_1_run_handle);

    // Wait for the nodes to start.
    integration_test_setup.await_alive(5000, 50).await;

    info!("Running integration test simulator.");

    let send_rpc_tx_fn = &mut |rpc_tx| {
        integration_test_setup.sequencer_0.add_tx_http_client.assert_add_tx_success(rpc_tx)
    };

    const ACCOUNT_ID_0: AccountId = 0;
    let n_txs = 50;
    let sender_address = tx_generator.account_with_id(ACCOUNT_ID_0).sender_address();
    info!("Sending {n_txs} txs.");
    let tx_hashes = send_account_txs(tx_generator, ACCOUNT_ID_0, n_txs, send_rpc_tx_fn).await;
    assert_eq!(tx_hashes.len(), n_txs);

    info!("Awaiting until {EXPECTED_BLOCK_NUMBER} blocks have been created.");

    let (batcher_storage_reader, _) =
        papyrus_storage::open_storage(integration_test_setup.sequencer_0.batcher_storage_config)
            .expect("Failed to open batcher's storage");

    await_block(5000, EXPECTED_BLOCK_NUMBER, 30, &batcher_storage_reader)
        .await
        .expect("Block number should have been reached.");

    info!("Shutting node 0 down.");
    node_0_run_task.abort();
    let res = node_0_run_task.await;
    assert!(
        res.expect_err("Node 0 should have been stopped.").is_cancelled(),
        "Node 0 should have been stopped."
    );
    info!("Shutting node 1 down.");
    node_1_run_task.abort();
    let res = node_1_run_task.await;
    assert!(
        res.expect_err("Node 1 should have been stopped.").is_cancelled(),
        "Node 1 should have been stopped."
    );

    info!("Verifying tx sender account nonce.");
    let expected_nonce_value = n_txs + 1;
    let expected_nonce =
        Nonce(Felt::from_hex_unchecked(format!("0x{:X}", expected_nonce_value).as_str()));
    let nonce = get_account_nonce(&batcher_storage_reader, sender_address);
    assert_eq!(nonce, expected_nonce);
}
